import { test } from "node:test";
import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const run = promisify(execFile);

/** 用临时构建和命令替身运行编排脚本，不启动服务或执行构建。 */
async function fixture(t, { synced = false, missingAsset = false, running = false, oldBuild = false, persistent = false } = {}) {
  const root = await mkdtemp(join(tmpdir(), "erp-ensure-services-test-"));
  t.after(async () => {
    if (persistent) {
      try { process.kill(Number(await readFile(join(root, "actual-node.pid"), "utf8")), "SIGTERM"); } catch {}
    }
    await rm(root, { recursive: true, force: true });
  });
  for (const dir of ["scripts", "logs", "bin", "erp-client/public", "erp-client/.next/static/chunks",
    "erp-client/.next/standalone/.next/static/chunks", "erp-client/.next/standalone/public"]) {
    await mkdir(join(root, dir), { recursive: true });
  }
  await copyFile(new URL("ensure-services.sh", import.meta.url), join(root, "scripts/ensure-services.sh"));
  const client = join(root, "erp-client");
  const standalone = join(client, ".next/standalone");
  await writeFile(join(client, "public/favicon.ico"), "icon");
  await writeFile(join(standalone, "public/favicon.ico"), "icon");
  await writeFile(join(client, ".next/static/chunks/current.js"), "current build");
  if (!missingAsset) await writeFile(join(standalone, ".next/static/chunks/current.js"), "current build");
  await writeFile(join(standalone, "server.js"), "// fixture");
  await writeFile(join(client, ".next/BUILD_ID"), "current-build\n");
  if (synced) await writeFile(join(standalone, ".next/e2e-assets-build-id"), "current-build\n");
  if (running) {
    await writeFile(join(root, "ready"), "");
    await writeFile(join(root, "logs/next-e2e.build-id"), oldBuild ? "old-build\n" : "current-build\n");
  }
  const executable = (path, text) => writeFile(join(root, path), text, { mode: 0o755 });
  await executable("scripts/restart-backend.sh", `#!/usr/bin/env bash
printf '%s\\n' check build run kill > "$ERP_TEST_ROOT/backend-actions"
`);
  await executable("bin/npm", "#!/usr/bin/env bash\necho 'unexpected npm build' >&2\nexit 99\n");
  await executable("bin/node", `#!/usr/bin/env bash
printf '%s\\n' "$$" > "$ERP_TEST_ROOT/actual-node.pid"
touch "$ERP_TEST_ROOT/ready"
${persistent ? "exec sleep 30" : "exit 0"}
`);
  await executable("bin/curl", `#!/usr/bin/env bash
for url; do :; done
printf '%s\\n' "$url" >> "$ERP_TEST_ROOT/curl-requests"
if [[ "$url" == http://127.0.0.1:10001/health ]]; then
  [[ "$ERP_TEST_BACKEND_HEALTHY" == 1 ]]
  exit $?
fi
[[ -f "$ERP_TEST_ROOT/ready" ]] || exit 22
if [[ "$ERP_TEST_PERSISTENT" == 1 && -f "$ERP_TEST_ROOT/actual-node.pid" ]]; then
  pid="$(cat "$ERP_TEST_ROOT/actual-node.pid")"
  status="$(ps -p "$pid" -o stat=)" || exit 22
  [[ "$status" != *Z* ]] || exit 22
fi
if [[ "$url" == */_next/static/* && "$ERP_TEST_ASSET_FAILURE" == 1 ]]; then exit 22; fi
exit 0
`);
  const execute = (extraEnv = {}) => run("bash", [join(root, "scripts/ensure-services.sh")], {
    env: { ...process.env, PATH: `${join(root, "bin")}:${process.env.PATH}`,
      E2E_FRONTEND: "prod", E2E_FRONT_BUILD: "0", E2E_SKIP_BACKEND: "0", ERP_TEST_ROOT: root,
      ERP_TEST_BACKEND_HEALTHY: "1",
      ERP_TEST_ASSET_FAILURE: "0", ERP_TEST_PERSISTENT: persistent ? "1" : "0", ...extraEnv }, timeout: 20000,
  }).then((value) => ({ ...value, code: 0 }), (error) => error);
  return { root, client, standalone, execute };
}

test("外部 fresh build 没有 marker 时同步静态资源，不执行 npm build", async (t) => {
  const { root, standalone, execute } = await fixture(t, { missingAsset: true });
  const result = await execute();
  assert.equal(result.code, 0, result.stderr);
  assert.equal(await readFile(join(standalone, ".next/static/chunks/current.js"), "utf8"), "current build");
  assert.equal(await readFile(join(standalone, ".next/e2e-assets-build-id"), "utf8"), "current-build\n");
  assert.equal(await readFile(join(root, "logs/next-e2e.build-id"), "utf8"), "current-build\n");
});

test("marker 一致但静态文件缺失时重新同步", async (t) => {
  const { standalone, execute } = await fixture(t, { synced: true, missingAsset: true });
  const result = await execute();
  assert.equal(result.code, 0, result.stderr);
  assert.match(result.stdout, /同步前端 standalone 静态资源/);
  assert.equal(await readFile(join(standalone, ".next/static/chunks/current.js"), "utf8"), "current build");
});

test("资源和运行构建一致时复用现有生产服务", async (t) => {
  const { execute } = await fixture(t, { synced: true, running: true });
  const result = await execute();
  assert.equal(result.code, 0, result.stderr);
  assert.match(result.stdout, /构建未过期，复用/);
  assert.doesNotMatch(result.stdout, /同步前端|启动前端/);
});

test("当前构建与运行实例不一致时拒绝占用端口的陌生服务", async (t) => {
  const { execute } = await fixture(t, { synced: true, running: true, oldBuild: true });
  const result = await execute();
  assert.equal(result.code, 1);
  assert.match(result.stderr, /非本脚本启动的进程占用/);
  assert.doesNotMatch(result.stdout, /同步前端/);
});

test("HTML 可达但当前构建 JS 失败时提前失败", async (t) => {
  const { execute } = await fixture(t, { synced: true });
  const result = await execute({ ERP_TEST_ASSET_FAILURE: "1" });
  assert.equal(result.code, 1);
  assert.match(result.stderr, /当前构建的静态 JS 不可达/);
});

test("PID 记录实际服务进程，服务在启动脚本结束后独立存活", async (t) => {
  const { root, execute } = await fixture(t, { synced: true, persistent: true });
  const result = await execute();
  assert.equal(result.code, 0, result.stderr);
  const pid = Number(await readFile(join(root, "logs/next-e2e.pid"), "utf8"));
  assert.equal(pid, Number(await readFile(join(root, "actual-node.pid"), "utf8")));
  assert.doesNotThrow(() => process.kill(pid, 0));
  const { stdout } = await run("ps", ["-p", String(pid), "-o", "pgid="]);
  assert.equal(Number(stdout.trim()), pid, "服务应位于独立进程组");
});

test("重新同步时关闭旧 PID 并登记新的服务 PID", async (t) => {
  const { root, client, execute } = await fixture(t, { synced: true, persistent: true });
  const first = await execute();
  assert.equal(first.code, 0, first.stderr);
  const oldPid = Number(await readFile(join(root, "logs/next-e2e.pid"), "utf8"));
  await writeFile(join(client, ".next/BUILD_ID"), "next-build\n");
  const second = await execute();
  assert.equal(second.code, 0, second.stderr);
  const newPid = Number(await readFile(join(root, "logs/next-e2e.pid"), "utf8"));
  assert.notEqual(newPid, oldPid);
  assert.equal(newPid, Number(await readFile(join(root, "actual-node.pid"), "utf8")));
  const status = await run("ps", ["-p", String(oldPid), "-o", "stat="]).then((r) => r.stdout.trim(), () => "");
  assert.ok(!status || status.startsWith("Z"), `旧服务仍在运行：${oldPid} ${status}`);
  assert.equal(await readFile(join(root, "logs/next-e2e.build-id"), "utf8"), "next-build\n");
});

test("仅准备前端时不探测或管理默认后端，继续完成静态资源与实例准备", async (t) => {
  const { root, standalone, execute } = await fixture(t, { missingAsset: true });
  await writeFile(join(root, "logs/web-api.pid"), String(process.pid));
  const result = await execute({ E2E_SKIP_BACKEND: "1", ERP_TEST_BACKEND_HEALTHY: "0" });
  assert.equal(result.code, 0, result.stderr);
  assert.match(result.stdout, /跳过默认后端准备/);
  assert.equal(await readFile(join(standalone, ".next/static/chunks/current.js"), "utf8"), "current build");
  assert.equal(await readFile(join(root, "logs/web-api.pid"), "utf8"), String(process.pid));
  assert.doesNotMatch(await readFile(join(root, "curl-requests"), "utf8"), /:10001/);
  await assert.rejects(readFile(join(root, "backend-actions")), { code: "ENOENT" });
});

test("仅准备前端同时适用于 next dev 复用", async (t) => {
  const { root, execute } = await fixture(t, { running: true });
  const result = await execute({ E2E_SKIP_BACKEND: "1", E2E_FRONTEND: "dev", ERP_TEST_BACKEND_HEALTHY: "0" });
  assert.equal(result.code, 0, result.stderr);
  assert.match(result.stdout, /next dev 已启动/);
  assert.doesNotMatch(await readFile(join(root, "curl-requests"), "utf8"), /:10001/);
  await assert.rejects(readFile(join(root, "backend-actions")), { code: "ENOENT" });
});

test("未启用跳过开关时继续探测并准备未启动的默认后端", async (t) => {
  const { root, execute } = await fixture(t, { synced: true, running: true });
  const result = await execute({ ERP_TEST_BACKEND_HEALTHY: "0" });
  assert.equal(result.code, 0, result.stderr);
  assert.match(result.stdout, /后端健康检查未通过/);
  assert.match(await readFile(join(root, "curl-requests"), "utf8"), /:10001\/health/);
  assert.equal(await readFile(join(root, "backend-actions"), "utf8"), "check\nbuild\nrun\nkill\n");
});
