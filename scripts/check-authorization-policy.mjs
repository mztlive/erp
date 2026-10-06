#!/usr/bin/env node
/** 授权 CLI 传输契约检查。只连接本进程 HTTP 替身，不连接 ERP 或数据库。 */
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const directory = await mkdtemp(join(tmpdir(), "erp-policy-cli-"));
const script = fileURLToPath(new URL("./authorization-policy.mjs", import.meta.url));
const document = { version: "1.0", roles: [], bindings: [], data_scopes: [] };
const plan = { document, policy_version: 4, review_hash: `sha256-v1:${"a".repeat(64)}`, changes: [], policy_notes: [] };
const calls = [];
let mode = "ok";
const server = createServer(async (request, response) => {
  let data = "";
  for await (const part of request) data += part;
  calls.push({ path: request.url, auth: request.headers.authorization, body: JSON.parse(data) });
  if (mode === "redirect") {
    response.writeHead(302, { location: "/redirect-target" });
    response.end();
    return;
  }
  if (mode === "invalid") {
    response.writeHead(200);
    response.end("invalid");
    return;
  }
  response.setHeader("content-type", "application/json");
  if (["missing-data", "null", "empty-receipt"].includes(mode)) {
    const envelope = mode === "null" ? null : { success: true, ...(mode === "empty-receipt" ? { data: {} } : {}) };
    response.end(JSON.stringify(envelope));
    return;
  }
  if (mode === "forbidden") {
    response.writeHead(403);
    response.end(JSON.stringify({ success: false, code: "FORBIDDEN", errorMessage: "拒绝" }));
    return;
  }
  const result = request.url.endsWith("/apply")
    ? { command_id: "command-1", policy_version: 5, change_count: 1, replayed: false }
    : plan;
  response.end(JSON.stringify({ success: true, data: result }));
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const base = `http://127.0.0.1:${server.address().port}`;

/** 子进程始终覆盖真实环境中的服务地址和凭证。 */
function run(args, env = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [script, ...args], {
      cwd: directory,
      env: { ...process.env, ERP_API_BASE: base, ERP_ACCESS_TOKEN: "local-fixture-token", ...env },
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (data) => { stdout += data; });
    child.stderr.on("data", (data) => { stderr += data; });
    child.on("error", reject);
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}

try {
  await writeFile(join(directory, "policy.json"), JSON.stringify(document));
  let result = await run(["preview", "--file", "policy.json", "--out", "plan.json"]);
  assert.equal(result.code, 0);
  assert.equal(calls.at(-1).path, "/admin/authorization-policies/preview");
  assert.deepEqual(calls.at(-1).body, document);
  assert.equal((await stat(join(directory, "plan.json"))).mode & 0o777, 0o600);

  result = await run(["apply", "--plan", "plan.json", "--operation-key", "change-1"]);
  assert.equal(result.code, 0);
  assert.deepEqual(calls.at(-1).body, {
    document, expected_policy_version: 4, review_hash: plan.review_hash, idempotency_key: "change-1",
  });
  assert.equal(calls.at(-1).auth, "Bearer local-fixture-token");

  await writeFile(join(directory, "large-plan.json"), JSON.stringify({ ...plan, policy_notes: ["x".repeat(1200000)] }));
  result = await run(["apply", "--plan", "large-plan.json", "--operation-key", "change-2"]);
  assert.equal(result.code, 0);

  result = await run(["preview", "--file", "policy.json", "--out", "plan.json"]);
  assert.equal(result.code, 1);
  assert.match(result.stderr, /API 已成功/);
  assert.deepEqual(JSON.parse(result.stdout), plan);
  assert.deepEqual(JSON.parse(await readFile(join(directory, "plan.json"), "utf8")), plan);

  result = await run(["export", "--users", "alice,bob", "--roles", "reader", "--out", "exported.json"]);
  assert.equal(result.code, 0);
  assert.deepEqual(calls.at(-1).body, { user_ids: ["alice", "bob"], role_ids: ["reader"] });
  result = await run(["preview", "--file", "exported.json"]);
  assert.equal(result.code, 0);
  assert.deepEqual(calls.at(-1).body, document);

  let count = calls.length;
  result = await run(["apply", "--plan", "plan.json"]);
  assert.equal(result.code, 1);
  assert.equal(calls.length, count);
  result = await run(["validate", "--file", "policy.json"], { ERP_ACCESS_TOKEN: "" });
  assert.equal(result.code, 1);
  assert.equal(calls.length, count);

  mode = "forbidden";
  result = await run(["preview", "--file", "policy.json"]);
  assert.equal(result.code, 1);
  assert.match(result.stderr, /403.*FORBIDDEN/);

  mode = "redirect";
  count = calls.length;
  result = await run(["apply", "--plan", "plan.json", "--operation-key", "same-key"]);
  assert.equal(result.code, 1);
  assert.equal(calls.length, count + 1);
  assert.match(result.stderr, /提交状态未知/);

  mode = "invalid";
  result = await run(["apply", "--plan", "plan.json", "--operation-key", "same-key"]);
  assert.equal(result.code, 1);
  assert.match(result.stderr, /提交状态未知/);
  assert(!result.stderr.includes("local-fixture-token"));
  for (const invalidEnvelope of ["missing-data", "null", "empty-receipt"]) {
    mode = invalidEnvelope;
    result = await run(["apply", "--plan", "plan.json", "--operation-key", "same-key"]);
    assert.equal(result.code, 1, invalidEnvelope);
    assert.match(result.stderr, /提交状态未知/);
  }
  process.stdout.write("授权 CLI：14 项传输契约检查通过；未连接真实 ERP、MongoDB 或 S3。\n");
} finally {
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
  await rm(directory, { recursive: true, force: true });
}
