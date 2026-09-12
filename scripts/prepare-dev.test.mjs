import { test } from "node:test";
import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
import { promisify } from "node:util";
import { execFile } from "node:child_process";

const run = promisify(execFile);

/** 在临时目录运行真实启动与清理脚本，以替身隔离清库、种子、构建和健康接口。 */
async function fixture(t, scenario) {
  const root = await mkdtemp(join(tmpdir(), "erp-prepare-test-"));
  const foreign = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], { stdio: "ignore" });
  t.after(async () => {
    foreign.kill();
    try {
      const pid = Number(await readFile(join(root, "started.pid"), "utf8"));
      try { process.kill(pid, "SIGKILL"); } catch {}
    } catch {}
    await rm(root, { recursive: true, force: true });
  });
  for (const dir of ["scripts", "backend", "logs", "bin", "target/debug"]) {
    await mkdir(join(root, dir), { recursive: true });
  }
  for (const name of ["prepare-dev.sh", "restart-backend.sh"]) {
    await copyFile(new URL(name, import.meta.url), join(root, "scripts", name));
  }
  const executable = async (path, text) => writeFile(join(root, path), text, { mode: 0o755 });
  await executable("bin/cargo", '#!/usr/bin/env bash\nprintf \'{"target_directory":"%s/target"}\\n\' "$ERP_TEST_ROOT"\n');
  await executable("bin/pgrep", "#!/usr/bin/env bash\nexit 1\n");
  await executable("bin/curl", `#!/usr/bin/env bash
if [[ "$ERP_TEST_SCENARIO" == startup ]]; then
  cp "$ERP_WEB_API_OWNED_PID_FILE" "$ERP_TEST_ROOT/started.pid"
  kill -TERM "$PPID"
  exit 1
fi
exit 0
`);
  await executable("target/debug/web-api", "#!/usr/bin/env bash\nexec python3 -c 'import time; time.sleep(60)'\n");
  await executable("scripts/reset-db.sh", `#!/usr/bin/env bash
set -euo pipefail
[[ "$ERP_RESET_ONLY" == 0 && "$ERP_RESET_INCLUDE_CATALOG" == 1 ]]
[[ -n "$ERP_WEB_API_OWNED_PID_FILE" ]]
if [[ "$ERP_TEST_SCENARIO" == before ]]; then exit 23; fi
bash "$(dirname "$0")/restart-backend.sh"
cp "$ERP_WEB_API_OWNED_PID_FILE" "$ERP_TEST_ROOT/started.pid"
if [[ "$ERP_TEST_SCENARIO" == after ]]; then exit 24; fi
if [[ "$ERP_TEST_SCENARIO" == replaced ]]; then
  printf '%s\\n' "$ERP_TEST_FOREIGN_PID" > "$ERP_TEST_ROOT/logs/web-api.pid"
fi
`);
  await writeFile(join(root, "logs/web-api.pid"), String(foreign.pid));
  const result = await run("bash", [join(root, "scripts/prepare-dev.sh")], {
    env: { ...process.env, PATH: `${join(root, "bin")}:${process.env.PATH}`,
      E2E_RESET: "1", ERP_RESET_ONLY: "1", ERP_TEST_ROOT: root,
      ERP_TEST_SCENARIO: scenario, ERP_TEST_FOREIGN_PID: String(foreign.pid) },
    timeout: 25000,
  }).then((value) => ({ ...value, code: 0 }), (error) => error);
  assert.doesNotThrow(() => process.kill(foreign.pid, 0), "不得关闭其他进程");
  return { root, result, foreign };
}

/** 确认进程已结束，兼容尚未被 init 回收的僵尸进程。 */
async function assertStopped(root) {
  const pid = (await readFile(join(root, "started.pid"), "utf8")).trim();
  const status = await run("ps", ["-p", pid, "-o", "stat="]).then((r) => r.stdout.trim(), () => "");
  assert.ok(!status || status.startsWith("Z"), `本次服务仍在运行：${pid} ${status}`);
}

test("准备成功后关闭本次服务并删除对应共享 PID 文件", async (t) => {
  const { root, result } = await fixture(t, "success");
  assert.equal(result.code, 0, result.stderr);
  assert.match(result.stdout, /种子数据已插入，本次启动的 web-api 已关闭/);
  await assertStopped(root);
  await assert.rejects(readFile(join(root, "logs/web-api.pid")), { code: "ENOENT" });
});

test("种子失败后仍关闭本次服务，保留原退出码", async (t) => {
  const { root, result } = await fixture(t, "after");
  assert.equal(result.code, 24);
  assert.doesNotMatch(result.stdout, /准备完成/);
  await assertStopped(root);
});

test("启动前失败不关闭共享 PID 指向的已有进程", async (t) => {
  const { root, result, foreign } = await fixture(t, "before");
  assert.equal(result.code, 23);
  assert.equal(Number(await readFile(join(root, "logs/web-api.pid"), "utf8")), foreign.pid);
});

test("启动脚本在健康检查阶段退出时仍清理已创建服务", async (t) => {
  const { root, result } = await fixture(t, "startup");
  assert.equal(result.code, 143);
  assert.doesNotMatch(result.stdout, /准备完成/);
  await assertStopped(root);
});

test("共享 PID 已替换时仍只关闭本次服务并保留新记录", async (t) => {
  const { root, result, foreign } = await fixture(t, "replaced");
  assert.equal(result.code, 0, result.stderr);
  await assertStopped(root);
  assert.equal(Number(await readFile(join(root, "logs/web-api.pid"), "utf8")), foreign.pid);
});
