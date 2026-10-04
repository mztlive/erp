#!/usr/bin/env node
/**
 * 授权配置 CLI。仅调用正式认证 API；不读数据库配置、不自动重试写入。
 * Node.js 22+，凭证仅从 ERP_ACCESS_TOKEN 读取。
 */
import { readFile, stat, writeFile } from "node:fs/promises";
import { parseArgs } from "node:util";

const HELP = `用法：
  node scripts/authorization-policy.mjs validate --file policy.json
  node scripts/authorization-policy.mjs preview --file policy.json --out plan.json
  node scripts/authorization-policy.mjs apply --plan plan.json --operation-key change-20261004-01
  node scripts/authorization-policy.mjs export --users user-id --roles role-id --out export.json

环境：
  ERP_API_BASE      API 根地址，默认 http://127.0.0.1:10001
  ERP_ACCESS_TOKEN  已登录后台账号的访问 Token

--users / --roles 接受逗号分隔 ID。导出结果可直接传给 preview。
--out 仅创建新文件；已有文件不会被覆盖。
apply 必须使用审核过的预览文件；超时或结果未知时保留原计划和操作号。
`;

/** 只接受完整机器结果；缺失结果不能解释为授权已成功。 */
function validResult(command, value) {
  const record = (item) => item !== null && typeof item === "object" && !Array.isArray(item);
  const integer = (item) => Number.isSafeInteger(item) && item >= 0;
  if (!record(value) || !integer(value.policy_version)) return false;
  if (command === "apply") {
    return typeof value.command_id === "string" && value.command_id.length > 0
      && integer(value.change_count) && typeof value.replayed === "boolean";
  }
  const doc = value.document;
  if (!record(doc) || doc.version !== "1.0"
    || ![doc.roles, doc.bindings, doc.data_scopes, value.policy_notes].every(Array.isArray)) return false;
  return command === "export"
    || (typeof value.review_hash === "string" && /^sha256-v1:[0-9a-f]{64}$/i.test(value.review_hash)
      && Array.isArray(value.changes));
}

/** 协议不完整时保留应用结果未知语义，防止调用方重新分配操作号。 */
function protocolError(command, status) {
  return new Error(command === "apply"
    ? `应用响应不符合接口合同（HTTP ${status}），提交状态未知；请保留原计划和操作号`
    : `API 响应不符合授权接口合同（HTTP ${status}）`);
}

/** 读取有界 JSON 文件，避免把任意大型文件载入请求。 */
async function jsonFile(path) {
  if (!path) throw new Error("缺少输入文件");
  const metadata = await stat(path);
  if (!metadata.isFile() || metadata.size > 32 * 1024 * 1024) {
    throw new Error("输入须为不超过 32 MiB 的 JSON 文件");
  }
  return JSON.parse(await readFile(path, "utf8"));
}

/** 根据明确命令生成请求，应用始终使用原计划。 */
async function payload(command, values) {
  if (command === "validate" || command === "preview") {
    if (!values.file || values.plan || values.users || values.roles || values["operation-key"]) {
      throw new Error("validate/preview 仅接受 --file 和可选 --out");
    }
    const input = await jsonFile(values.file);
    return input.document ?? input;
  }
  if (command === "apply") {
    if (!values.plan || !values["operation-key"] || values.file || values.users || values.roles) {
      throw new Error("apply 必须提供 --plan 和 --operation-key");
    }
    const plan = await jsonFile(values.plan);
    if (!validResult("preview", plan)) {
      throw new Error("输入不是有效预览文件，请先执行 preview");
    }
    return {
      document: plan.document,
      expected_policy_version: plan.policy_version,
      review_hash: plan.review_hash,
      idempotency_key: values["operation-key"],
    };
  }
  if (command === "export") {
    if ((!values.users && !values.roles) || values.file || values.plan || values["operation-key"]) {
      throw new Error("export 必须明确提供 --users 或 --roles");
    }
    const ids = (value) => value ? value.split(",").map((id) => id.trim()) : [];
    return { user_ids: ids(values.users), role_ids: ids(values.roles) };
  }
  throw new Error("命令须为 validate、preview、apply 或 export");
}

/** 单次调用正式授权接口；禁止重定向及自动重试。 */
async function request(command, body) {
  const token = process.env.ERP_ACCESS_TOKEN?.trim();
  if (!token) throw new Error("请通过 ERP_ACCESS_TOKEN 提供当前操作人的访问 Token");
  const base = new URL(process.env.ERP_API_BASE ?? "http://127.0.0.1:10001");
  if (!["http:", "https:"].includes(base.protocol) || base.username || base.password || base.search || base.hash) {
    throw new Error("ERP_API_BASE 必须为不含凭证、查询参数或片段的 HTTP(S) 地址");
  }
  const url = new URL(`${base.pathname.replace(/\/$/, "")}/admin/authorization-policies/${command}`, base);
  const serialized = JSON.stringify(body);
  if (Buffer.byteLength(serialized) > 2 * 1024 * 1024) {
    throw new Error("授权请求超过 2 MiB，请缩小本次配置范围");
  }
  let response;
  try {
    response = await fetch(url, {
      method: "POST", redirect: "error", signal: AbortSignal.timeout(30000),
      headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
      body: serialized,
    });
  } catch {
    throw new Error(command === "apply"
      ? "未收到应用结果，提交状态未知；请保留原计划和操作号，核实服务状态后再查询或使用原请求重试"
      : "API 请求失败或超时，请检查服务地址和连接");
  }
  let envelope;
  try {
    envelope = await response.json();
  } catch {
    throw new Error(command === "apply"
      ? `应用响应无法解析（HTTP ${response.status}），提交状态未知；请保留原计划和操作号`
      : `API 返回了无效 JSON（HTTP ${response.status}）`);
  }
  if (!envelope || typeof envelope.success !== "boolean") throw protocolError(command, response.status);
  if (!response.ok || envelope.success !== true) {
    throw new Error(`HTTP ${response.status}: ${envelope.code ?? ""} ${envelope.errorMessage ?? "授权操作失败"}`.trim());
  }
  if (!validResult(command, envelope.data)) throw protocolError(command, response.status);
  return envelope.data;
}

/** CLI 主流程；所有机器结果写 stdout，错误写 stderr。 */
async function main() {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      file: { type: "string" }, plan: { type: "string" }, out: { type: "string" },
      users: { type: "string" }, roles: { type: "string" },
      "operation-key": { type: "string" }, help: { type: "boolean" },
    },
  });
  if (values.help) {
    process.stdout.write(HELP);
    return;
  }
  if (positionals.length !== 1) throw new Error(HELP);
  const command = positionals[0];
  const body = await payload(command, values);
  const result = await request(command, body);
  const output = `${JSON.stringify(result, null, 2)}\n`;
  if (values.out) {
    try {
      await writeFile(values.out, output, { encoding: "utf8", mode: 0o600, flag: "wx" });
    } catch {
      process.stdout.write(output);
      throw new Error("API 已成功，但结果文件未能创建；完整结果已输出，请另行保存");
    }
  } else {
    process.stdout.write(output);
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
