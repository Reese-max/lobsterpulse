"""Offline Issue #9 attention-policy spike. No app, shell or network integration."""

import argparse
import hashlib
import json
import pathlib
import re


SCHEMA_VERSION = 1
POLICY_VERSION = "issue9-synthetic-v1"
WINDOW_MS = 300_000
KINDS = frozenset({
    "COMPLETED", "WAITING", "RECOVERED", "ERROR", "QUOTA", "STALE",
    "NOT_MONITORED", "EXTERNAL_DEPENDENCY", "UNKNOWN",
})
FRESHNESS = frozenset({"FRESH", "STALE", "UNKNOWN"})
EVENT_FIELDS = frozenset({
    "schemaVersion", "eventId", "providerId", "sessionId", "correlationId", "kind",
    "observedAtMs", "sourceFreshness", "sourceContractVersion",
    "reasonCode", "payloadHash",
})


def _digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def validate_event(raw):
    """Reject missing fields and content-bearing extras rather than silently dropping them."""
    if not isinstance(raw, dict) or set(raw) != EVENT_FIELDS:
        raise ValueError(f"event must contain exactly {sorted(EVENT_FIELDS)}")
    if raw["schemaVersion"] != SCHEMA_VERSION:
        raise ValueError("unsupported event schema version")
    for field in ("eventId", "providerId", "sessionId", "correlationId", "sourceContractVersion"):
        if not isinstance(raw[field], str) or not re.fullmatch(r"[A-Za-z0-9_.:-]{1,128}", raw[field]):
            raise ValueError(f"invalid {field}")
    if raw["kind"] not in KINDS or raw["sourceFreshness"] not in FRESHNESS:
        raise ValueError("unknown event kind or freshness")
    if not isinstance(raw["observedAtMs"], int) or isinstance(raw["observedAtMs"], bool) or raw["observedAtMs"] < 0:
        raise ValueError("observedAtMs must be a nonnegative integer")
    if not isinstance(raw["reasonCode"], str) or not re.fullmatch(r"[A-Z][A-Z0-9_]{0,63}", raw["reasonCode"]):
        raise ValueError("reasonCode must be a bounded code, not free text")
    if not isinstance(raw["payloadHash"], str) or not re.fullmatch(r"[0-9a-f]{64}", raw["payloadHash"]):
        raise ValueError("payloadHash must be a SHA-256 hex digest")
    return dict(raw)


def preinvestigate(event):
    """Only return already-normalized metadata. No discovery or external reads."""
    event = validate_event(event)
    return {key: event[key] for key in (
        "providerId", "sessionId", "correlationId", "kind", "sourceFreshness", "reasonCode", "observedAtMs"
    )}


def _classification(event):
    freshness = event["sourceFreshness"]
    if freshness == "STALE" or event["kind"] == "STALE":
        return "NEEDS_DECISION", "SOURCE_STALE"
    if freshness == "UNKNOWN":
        return "NEEDS_DECISION", "SOURCE_FRESHNESS_UNKNOWN"
    return {
        "COMPLETED": ("INFO", "ROUTINE_COMPLETION"),
        "WAITING": ("NEEDS_DECISION", "WAITING_FOR_USER"),
        "RECOVERED": ("INFO", "UNMATCHED_RECOVERY"),
        "ERROR": ("CRITICAL", "CRITICAL_ERROR") if event["reasonCode"] == "CRITICAL_ERROR" else ("BLOCKING", "PROVIDER_ERROR"),
        "QUOTA": ("NEEDS_DECISION", "QUOTA_ATTENTION"),
        "STALE": ("NEEDS_DECISION", "SOURCE_STALE"),
        "NOT_MONITORED": ("NEEDS_DECISION", "SOURCE_NOT_MONITORED"),
        "EXTERNAL_DEPENDENCY": ("NEEDS_DECISION", "EXTERNAL_OWNER_REQUIRED"),
        "UNKNOWN": ("NEEDS_DECISION", "UNCLASSIFIED_SOURCE"),
    }[event["kind"]]


def _receipt(state, item, from_state, disposition, at_ms, actor="system"):
    event_ids = list(item["sourceEventIds"])
    payloads = {event["eventId"]: event["payloadHash"] for event in state["rawEvents"]}
    state["receipts"].append({
        "schemaVersion": SCHEMA_VERSION,
        "itemId": item["itemId"],
        "sourceEventIds": event_ids,
        "policyVersion": state["policyVersion"],
        "fromState": from_state,
        "toState": item["state"],
        "disposition": disposition,
        "actor": actor,
        "atMs": at_ms,
        "evidenceHash": _digest([state["policyVersion"], [(id_, payloads[id_]) for id_ in event_ids], disposition]),
    })


def replay(raw_events, policy_version=POLICY_VERSION, window_ms=WINDOW_MS):
    if not isinstance(policy_version, str) or not policy_version:
        raise ValueError("policy version is required")
    if not isinstance(window_ms, int) or window_ms <= 0:
        raise ValueError("correlation window must be positive")
    events = [validate_event(event) for event in raw_events]
    events.sort(key=lambda event: (event["observedAtMs"], event["eventId"]))
    seen = {}
    distinct = []
    for event in events:
        previous = seen.get(event["eventId"])
        if previous is not None:
            if previous != event:
                raise ValueError(f"conflicting duplicate eventId {event['eventId']}")
            continue
        seen[event["eventId"]] = event
        distinct.append(event)
    state = {"schemaVersion": SCHEMA_VERSION, "policyVersion": policy_version,
             "rawEvents": distinct, "items": [], "receipts": []}
    waiting = {}
    dedupe = {}
    for event in distinct:
        source_key = (event["providerId"], event["sessionId"], event["correlationId"])
        prior = waiting.get(source_key)
        if (event["kind"] == "RECOVERED" and event["sessionId"] != "UNKNOWN"
                and event["correlationId"] != "UNKNOWN"
                and event["sourceFreshness"] == "FRESH" and prior is not None
                and prior["state"] == "OPEN" and prior["sourceFreshness"] == "FRESH"
                and event["observedAtMs"] - prior["lastSeenAtMs"] <= window_ms):
            old_state = prior["state"]
            prior["sourceEventIds"].append(event["eventId"])
            prior["lastSeenAtMs"] = event["observedAtMs"]
            prior["state"] = "AUTO_RESOLVED"
            _receipt(state, prior, old_state, "FRESH_RECOVERY", event["observedAtMs"])
            waiting.pop(source_key, None)
            continue
        severity, why_now = _classification(event)
        # Unknown session or correlation identity cannot prove two events share a cause.
        correlation_key = (event["sessionId"], event["correlationId"])
        if "UNKNOWN" in correlation_key:
            correlation_key = (event["eventId"], event["eventId"])
        key = (event["providerId"], correlation_key, event["kind"], why_now, event["sourceFreshness"])
        prior = dedupe.get(key)
        if (prior is not None and prior["state"] == "OPEN"
                and event["observedAtMs"] - prior["lastSeenAtMs"] <= window_ms):
            prior["sourceEventIds"].append(event["eventId"])
            prior["lastSeenAtMs"] = event["observedAtMs"]
            _receipt(state, prior, "OPEN", "DEDUPED", event["observedAtMs"])
            continue
        item = {
            "schemaVersion": SCHEMA_VERSION,
            "itemId": "ai_" + _digest([policy_version, event["eventId"]])[:16],
            "providerId": event["providerId"], "sessionId": event["sessionId"],
            "correlationId": event["correlationId"],
            "severity": severity, "whyNow": why_now,
            "sourceFreshness": event["sourceFreshness"],
            "sourceEventIds": [event["eventId"]], "policyVersion": policy_version,
            "state": "OPEN", "snoozeUntilMs": None,
            "createdAtMs": event["observedAtMs"], "lastSeenAtMs": event["observedAtMs"],
        }
        state["items"].append(item)
        dedupe[key] = item
        if (event["kind"] == "WAITING" and event["sessionId"] != "UNKNOWN"
                and event["correlationId"] != "UNKNOWN" and event["sourceFreshness"] == "FRESH"):
            waiting[source_key] = item
        _receipt(state, item, "NEW", "CLASSIFIED", event["observedAtMs"])
    return state


def queue(state):
    """ACKED blocking/critical items remain visible until explicitly resolved."""
    return [item for item in state["items"] if item["severity"] != "INFO"
            and item["state"] in {"OPEN", "ACKED"}]


def decide(state, item_id, action, at_ms, until_ms=None):
    item = next((item for item in state["items"] if item["itemId"] == item_id), None)
    if item is None:
        raise ValueError("unknown attention item")
    before = item["state"]
    if action == "SNOOZE" and item["severity"] == "NEEDS_DECISION" and before == "OPEN" and isinstance(until_ms, int) and until_ms > at_ms:
        item["state"], item["snoozeUntilMs"] = "SNOOZED", until_ms
    elif action == "ACK" and item["severity"] in {"BLOCKING", "CRITICAL"} and before == "OPEN":
        item["state"] = "ACKED"
    elif action == "RESOLVE" and before in {"OPEN", "SNOOZED", "ACKED"} and item["severity"] != "INFO":
        item["state"], item["snoozeUntilMs"] = "RESOLVED", None
    else:
        raise ValueError("invalid or disallowed decision")
    _receipt(state, item, before, action, at_ms, actor="local-user")


def advance_time(state, now_ms):
    for item in state["items"]:
        if item["state"] == "SNOOZED" and item["snoozeUntilMs"] <= now_ms:
            item["state"], item["snoozeUntilMs"] = "OPEN", None
            _receipt(state, item, "SNOOZED", "SNOOZE_EXPIRED", now_ms)


def synthetic_demo():
    """A tiny content-free fixture for the command-line replay demonstration."""
    base = {"schemaVersion": SCHEMA_VERSION, "sourceFreshness": "FRESH",
            "sourceContractVersion": "synthetic-v1", "reasonCode": "UNKNOWN",
            "payloadHash": "0" * 64}
    rows = [
        ("wait-1", "claude", "s1", "c1", "WAITING", 1000, "UNKNOWN"),
        ("wait-2", "claude", "s1", "c1", "WAITING", 1100, "UNKNOWN"),
        ("recovery", "claude", "s1", "c1", "RECOVERED", 1200, "UNKNOWN"),
        ("completion", "codex", "s2", "c2", "COMPLETED", 1300, "UNKNOWN"),
        ("unavailable", "openab", "UNKNOWN", "UNKNOWN", "NOT_MONITORED", 1400, "UNKNOWN"),
        ("critical", "gemini", "s3", "c3", "ERROR", 1500, "CRITICAL_ERROR"),
    ]
    return [dict(base, eventId=id_, providerId=provider, sessionId=session,
                 correlationId=correlation, kind=kind, observedAtMs=when, reasonCode=reason)
            for id_, provider, session, correlation, kind, when, reason in rows]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture", nargs="?", type=pathlib.Path,
                        help="optional JSON event array; omit for the built-in synthetic fixture")
    args = parser.parse_args()
    events = json.loads(args.fixture.read_text(encoding="utf-8")) if args.fixture else synthetic_demo()
    state = replay(events)
    print(json.dumps({
        "policyVersion": state["policyVersion"], "rawEventCount": len(state["rawEvents"]),
        "itemCount": len(state["items"]), "humanQueueCount": len(queue(state)),
        "autoResolvedCount": sum(item["state"] == "AUTO_RESOLVED" for item in state["items"]),
        "queueReasons": [item["whyNow"] for item in queue(state)],
    }, sort_keys=True))


if __name__ == "__main__":
    main()
