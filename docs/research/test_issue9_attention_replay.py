"""Synthetic-only checks; these do not exercise the packaged desktop app."""

import builtins
import json
import random
import socket
import subprocess
import unittest
from unittest.mock import patch

from issue9_attention_replay import advance_time, decide, preinvestigate, queue, replay


def event(id_, kind, when, provider="claude", session="s1", correlation="c1", freshness="FRESH", reason="UNKNOWN"):
    return {
        "schemaVersion": 1, "eventId": id_, "providerId": provider,
        "sessionId": session, "correlationId": correlation,
        "kind": kind, "observedAtMs": when,
        "sourceFreshness": freshness, "sourceContractVersion": "synthetic-v1",
        "reasonCode": reason, "payloadHash": "0" * 64,
    }


class AttentionReplayTests(unittest.TestCase):
    def test_wait_recovery_retains_all_ids_and_dedupes(self):
        state = replay([event("w2", "WAITING", 1100), event("r1", "RECOVERED", 1200), event("w1", "WAITING", 1000)])
        self.assertEqual(len(state["items"]), 1)
        item = state["items"][0]
        self.assertEqual(item["state"], "AUTO_RESOLVED")
        self.assertEqual(item["sourceEventIds"], ["w1", "w2", "r1"])
        self.assertEqual(queue(state), [])
        self.assertEqual([receipt["disposition"] for receipt in state["receipts"]],
                         ["CLASSIFIED", "DEDUPED", "FRESH_RECOVERY"])

    def test_stale_unknown_and_unavailable_never_become_success(self):
        events = [
            event("wait", "WAITING", 1000),
            event("stale-recovery", "RECOVERED", 1100, freshness="STALE"),
            event("not-monitored", "NOT_MONITORED", 1200, provider="openab", session="s2"),
            event("external", "EXTERNAL_DEPENDENCY", 1300, provider="openab", session="s3"),
            event("unknown", "UNKNOWN", 1400, provider="gemini", session="UNKNOWN", freshness="UNKNOWN"),
        ]
        state = replay(events)
        self.assertEqual(len(queue(state)), 5)
        self.assertFalse(any(item["state"] == "AUTO_RESOLVED" for item in state["items"]))
        self.assertEqual({item["whyNow"] for item in queue(state)}, {
            "WAITING_FOR_USER", "SOURCE_STALE", "SOURCE_NOT_MONITORED",
            "EXTERNAL_OWNER_REQUIRED", "SOURCE_FRESHNESS_UNKNOWN",
        })

    def test_unknown_session_or_correlation_does_not_merge_and_conflicting_ids_fail(self):
        a = event("a", "ERROR", 1000, session="UNKNOWN")
        b = event("b", "ERROR", 1100, session="UNKNOWN")
        self.assertEqual(len(replay([a, b])["items"]), 2)
        c = event("c", "WAITING", 1200, correlation="UNKNOWN")
        d = event("d", "WAITING", 1300, correlation="UNKNOWN")
        e = event("e", "RECOVERED", 1400, correlation="UNKNOWN")
        self.assertEqual(len(replay([c, d, e])["items"]), 3)
        bad = dict(a, payloadHash="f" * 64)
        with self.assertRaisesRegex(ValueError, "conflicting duplicate eventId"):
            replay([a, bad])

    def test_cross_provider_events_do_not_share_a_local_correlation_id(self):
        # A provider-local correlation value is not evidence of a shared root cause.
        state = replay([event("claude-wait", "WAITING", 1000, provider="claude"),
                        event("codex-wait", "WAITING", 1100, provider="codex")])
        self.assertEqual(len(queue(state)), 2)

    def test_routine_completion_and_critical_ack_are_distinct(self):
        state = replay([event("done", "COMPLETED", 1000),
                        event("critical", "ERROR", 1100, reason="CRITICAL_ERROR")])
        self.assertEqual(len(queue(state)), 1)
        critical = queue(state)[0]
        self.assertEqual(critical["severity"], "CRITICAL")
        self.assertEqual(state["items"][0]["severity"], "INFO")
        with self.assertRaisesRegex(ValueError, "disallowed"):
            decide(state, critical["itemId"], "SNOOZE", 1200, 5000)
        with self.assertRaisesRegex(ValueError, "disallowed"):
            decide(state, critical["itemId"], "DISMISS", 1200)
        decide(state, critical["itemId"], "ACK", 1200)
        self.assertEqual(queue(state)[0]["state"], "ACKED")
        decide(state, critical["itemId"], "RESOLVE", 1300)
        self.assertEqual(queue(state), [])
        self.assertEqual(state["receipts"][-2]["actor"], "local-user")

    def test_snooze_expiry_and_resolve_survive_serialized_restart(self):
        state = replay([event("wait", "WAITING", 1000)])
        item_id = queue(state)[0]["itemId"]
        decide(state, item_id, "SNOOZE", 1100, 2000)
        restored = json.loads(json.dumps(state, sort_keys=True))
        self.assertEqual(queue(restored), [])
        advance_time(restored, 1999)
        self.assertEqual(queue(restored), [])
        advance_time(restored, 2000)
        self.assertEqual(len(queue(restored)), 1)
        decide(restored, item_id, "RESOLVE", 2100)
        advance_time(restored, 3000)
        self.assertEqual(queue(restored), [])
        self.assertEqual([receipt["disposition"] for receipt in restored["receipts"][-3:]],
                         ["SNOOZE", "SNOOZE_EXPIRED", "RESOLVE"])

    def test_no_content_or_external_effects_in_bounded_preinvestigation(self):
        safe = event("safe", "WAITING", 1000)
        with self.assertRaisesRegex(ValueError, "exactly"):
            replay([dict(safe, prompt="secret content")])
        with self.assertRaisesRegex(ValueError, "invalid sessionId"):
            replay([dict(safe, sessionId="raw prompt text")])
        with patch.object(builtins, "open", side_effect=AssertionError("file read")), \
             patch.object(subprocess, "run", side_effect=AssertionError("shell")), \
             patch.object(socket, "socket", side_effect=AssertionError("network")):
            self.assertEqual(preinvestigate(safe)["reasonCode"], "UNKNOWN")
            self.assertEqual(len(replay([safe])["items"]), 1)

    def test_fixed_noisy_100_event_replay_is_deterministic_and_traceable(self):
        events = [event(f"done-{i:03d}", "COMPLETED", i, session=f"done-{i}") for i in range(80)]
        events += [event(f"wait-{i:03d}", "WAITING", 100 + i, session="shared") for i in range(10)]
        events += [event(f"error-{i:03d}", "ERROR", 200 + i, session=f"error-{i}") for i in range(8)]
        events += [event("resolved-wait", "WAITING", 300, session="resolved"),
                   event("resolved-recovery", "RECOVERED", 301, session="resolved")]
        first = replay(events)
        shuffled = list(events)
        random.Random(9).shuffle(shuffled)
        self.assertEqual(first, replay(shuffled))
        self.assertEqual(len(first["rawEvents"]), 100)
        self.assertEqual(len(queue(first)), 9)
        self.assertEqual(sum(item["state"] == "AUTO_RESOLVED" for item in first["items"]), 1)
        self.assertEqual({event_id for item in first["items"] for event_id in item["sourceEventIds"]},
                         {event_["eventId"] for event_ in events})
        old_receipts = json.dumps(first["receipts"], sort_keys=True)
        revised = replay(events, policy_version="issue9-synthetic-v2")
        self.assertNotEqual(first["items"][0]["itemId"], revised["items"][0]["itemId"])
        self.assertEqual(old_receipts, json.dumps(first["receipts"], sort_keys=True))


if __name__ == "__main__":
    unittest.main()
