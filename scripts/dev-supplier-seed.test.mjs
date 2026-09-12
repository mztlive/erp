import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { SUPPLIERS, PRODUCTS } from "./seed-dev-catalog.mjs";
import { COMPANY_PARTY, SUPPLIER_SCENARIOS, ensureCompanyParty, ensureSupplier, supplierCommand, supplierContract, verifySupplier, verifyOffering } from "./dev-supplier-seed.mjs";

const today = "2026-09-10";
const company = { id: "company-1", party_no: "FSY", version: 2, status: "active",
  legal_name: COMPANY_PARTY.legalName, short_name: COMPANY_PARTY.shortName,
  aliases: COMPANY_PARTY.aliases, unified_credit_code: COMPANY_PARTY.unifiedCreditCode };
const specs = [...SUPPLIERS, ...SUPPLIER_SCENARIOS];

function supplierDetail(spec, date = today) {
  const command = supplierCommand(spec, company.id, date);
  return { id: "supplier-1", supplier_no: spec.supplierNo, status: "active", party_status: "active",
    current_profile: command,
    capabilities: spec.capabilityCodes.map((code) => ({ id: `cap-${code}`, capability_code: code, status: "active" })),
    qualifications: [{ ...command.qualifications[0], status: "active", capability_ids: spec.capabilityCodes.map((code) => `cap-${code}`) }],
  };
}

test("公司首次通过专用接口创建；包含别名、不混入旧接口字段", async () => {
  const calls = [];
  const result = await ensureCompanyParty(async (method, path, options) => {
    calls.push({ method, path, body: options.body });
    return method === "GET" ? { items: [] } : company;
  }, "token");
  assert.equal(result.id, company.id);
  const write = calls.find((call) => call.method === "POST");
  assert.equal(write.path, "/admin/companies");
  assert.deepEqual(write.body.aliases, ["福尚云开发示例", "朱太帅", "科技"]);
  assert.equal(write.body.change_reason, undefined);
  assert.equal(write.body.version, null);
});

test("公司重跑只回读；补种子别名时保留现有别名和版本", async () => {
  await ensureCompanyParty(async (method) => {
    assert.equal(method, "GET"); return { items: [company] };
  }, "token");
  await ensureCompanyParty(async (method, path, options) => {
    if (method === "GET") return { items: [{ ...company, aliases: ["人工别名"] }] };
    assert.equal(method, "PUT");
    assert.equal(options.body.version, company.version);
    assert.deepEqual(options.body.aliases, ["人工别名", ...COMPANY_PARTY.aliases]);
    return company;
  }, "token");
});

test("旧种子公司补齐模板主体别名后重跑零写入，保留原身份和人工资料", async () => {
  let saved = { ...company, short_name: "人工简称", aliases: ["福尚云开发示例", "人工别名"] };
  let writes = 0;
  const call = async (method, path, options) => {
    if (method === "GET") return { items: [saved] };
    assert.equal(method, "PUT");
    assert.equal(path, `/admin/companies/${company.id}`);
    assert.deepEqual(options.body, {
      party_no: company.party_no, version: company.version,
      legal_name: company.legal_name, short_name: "人工简称",
      aliases: ["福尚云开发示例", "人工别名", "朱太帅", "科技"],
      unified_credit_code: company.unified_credit_code, status: "active",
    });
    writes++;
    saved = { ...saved, ...options.body, version: saved.version + 1 };
    return saved;
  };
  const first = await ensureCompanyParty(call, "token");
  assert.equal(first.id, company.id);
  assert.deepEqual(await ensureCompanyParty(call, "token"), first);
  assert.equal(writes, 1);
});

test("旧普通主体、停用公司或资料冲突均停止，不创建重复身份", async () => {
  await assert.rejects(ensureCompanyParty(async (method, path) => {
    assert.equal(method, "GET");
    return { items: path.startsWith("/admin/parties") ? [{ party_no: "FSY" }] : [] };
  }, "token"), /公司角色映射/);
  for (const patch of [{ status: "disabled" }, { unified_credit_code: "changed" }]) {
    await assert.rejects(ensureCompanyParty(async (method) => {
      assert.equal(method, "GET"); return { items: [{ ...company, ...patch }] };
    }, "token"), /不会覆盖/);
  }
});

test("种子覆盖五个自然周期、预付、现结，以及合同有效/未核实/过期", () => {
  assert.deepEqual(new Set(specs.map((row) => row.settlementMode)), new Set(["prepayment", "cash_settlement", "weekly", "monthly", "quarterly", "half_yearly", "yearly"]));
  assert.deepEqual(new Set(specs.map((row) => row.contractState)), new Set(["valid", "unverified", "expired"]));
  for (const spec of specs) {
    const body = supplierCommand(spec, company.id, today);
    assert.equal(body.invoice_tax_rate, undefined);
    assert.deepEqual(body.invoice_tax_rates, spec.invoiceTaxRates);
    assert.equal(body.signing_entity_party_id, company.id);
    assert.equal(body.payment_entity_party_id, company.id);
    assert.deepEqual(body.qualifications[0].capability_codes, spec.capabilityCodes);
    if (spec.contractState === "unverified") assert.equal(body.qualifications[0].valid_from, null);
    if (spec.contractState === "expired") assert.ok(body.qualifications[0].valid_to < today);
    if (spec.contractState === "valid") assert.ok(body.qualifications[0].valid_from <= today && body.qualifications[0].valid_to > today);
  }
  assert.equal(new Set(specs.map((row) => row.supplierNo)).size, specs.length);
  const tea = PRODUCTS.filter((row) => row.supplierNo === "SUP-HZSF");
  assert.deepEqual(tea.map((row) => row.inputTaxRate ?? "0.13"), ["0.09", "0.13"]);
});

test("合同日期偏移覆盖跨年、闰年，未知起始日期保持为空", () => {
  assert.equal(supplierContract({ ...SUPPLIERS[0], contractState: "expired" }, "2024-03-01").valid_to, "2024-02-29");
  assert.equal(supplierContract({ ...SUPPLIERS[0], contractState: "expired" }, "2026-01-01").valid_to, "2025-12-31");
});

test("已有供应商逐项核对商务和合同状态，不同资料不得假报成功", async () => {
  for (const spec of specs) await verifySupplier(async () => supplierDetail(spec), "token", { id: "supplier-1" }, spec, company.id, today);
  for (const mutate of [
    (row) => { row.current_profile.payment_term_snapshot = "POSTPAY_NET15"; },
    (row) => { row.current_profile.invoice_tax_rates = ["0.06"]; },
    (row) => { row.current_profile.signing_entity_party_id = "wrong"; },
    (row) => { row.qualifications = []; },
    (row) => { row.qualifications[0].capability_ids = []; },
    (row) => { row.qualifications[0].valid_to = "2025-01-01"; },
  ]) {
    const detail = supplierDetail(SUPPLIERS[0]); mutate(detail);
    await assert.rejects(verifySupplier(async () => detail, "token", { id: "supplier-1" }, SUPPLIERS[0], company.id, today), /最新种子不一致/);
  }
});

test("供应商首次新建后核对，重复运行零新增，HTTP 失败保留失败", async () => {
  const spec = SUPPLIERS[0]; let detail; let writes = 0;
  const call = async (method, path, options) => {
    if (path.startsWith("/admin/suppliers?")) return { items: detail ? [{ id: detail.id, supplier_no: spec.supplierNo }] : [] };
    if (method === "POST") { writes++; detail = supplierDetail(spec, options.body.effective_from); return { supplier_id: detail.id, supplier_no: spec.supplierNo }; }
    return detail;
  };
  const first = await ensureSupplier(call, "token", spec, company.id);
  assert.deepEqual(await ensureSupplier(call, "token", spec, company.id), first);
  assert.equal(writes, 1);
  await assert.rejects(ensureSupplier(async () => { throw new Error("读取失败"); }, "token", spec, company.id), /读取失败/);
});

test("与 Rust 校验使用同一组实际种子请求，禁止夹具与生成器漂移", async () => {
  const fixture = JSON.parse(await readFile(new URL("./fixtures/dev-supplier-profiles.json", import.meta.url), "utf8"));
  assert.deepEqual(fixture, specs.map((spec) => supplierCommand(spec, company.id, today)));
});

test("供给税率必须按 SKU 回读一致，不能把旧 13% 当作新 9% 样例", () => {
  const row = { supplier_id: "supplier-1", sku_id: "sku-1", status: "ACTIVE", availability_status: "AVAILABLE",
    valid_from: "2026-01-01", valid_to: null, input_tax_rate: "0.090000" };
  assert.equal(verifyOffering(row, PRODUCTS[0], "supplier-1", "sku-1", today), row);
  for (const patch of [{ input_tax_rate: "0.130000" }, { availability_status: "UNAVAILABLE" }, { valid_to: "2025-01-01" }]) {
    assert.throws(() => verifyOffering({ ...row, ...patch }, PRODUCTS[0], "supplier-1", "sku-1", today), /最新种子不一致/);
  }
  assert.doesNotThrow(() => verifyOffering({ ...row, input_tax_rate: "0.000000" }, { ...PRODUCTS[0], inputTaxRate: "0" }, "supplier-1", "sku-1", today));
});
