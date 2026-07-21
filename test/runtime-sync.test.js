// runtime/ 是本機執行期檔案的版控副本，只有人記得跑同步時才不會腐爛。
// 這條測試把「有沒有漂移」變成跑測試就會知道的事——今天已經漏過一次
// （備份檔命名 .bak2- 不符過濾規則，差點被同步進 repo）。
//
// 只在這台開發機上有意義：本機沒有 ~/.lobsterpulse/scripts（CI、別台機器）
// 就跳過，不是失敗。

import { test } from "node:test";
import assert from "node:assert";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const repo = path.resolve(import.meta.dirname, "..");
const localScripts = path.join(os.homedir(), ".lobsterpulse", "scripts");

test("runtime/ 與本機執行期檔案沒有漂移", { skip: !fs.existsSync(localScripts) && "本機無 ~/.lobsterpulse/scripts" }, () => {
  try {
    execFileSync(process.execPath, [path.join(repo, "runtime", "sync-from-local.mjs"), "--check"], {
      encoding: "utf8",
      stdio: "pipe",
    });
  } catch (e) {
    assert.fail(
      `runtime/ 已過期，請跑 node runtime/sync-from-local.mjs\n${e.stderr || e.stdout || e.message}`
    );
  }
});
