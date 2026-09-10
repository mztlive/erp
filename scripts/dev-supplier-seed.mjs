/** 开发供应商种子合同；可注入 HTTP 调用进行纯内存验证。 */
export const COMPANY_PARTY = {
  partyNo: "FSY",
  legalName: "北京福尚云科技有限公司",
  shortName: "福尚云",
  aliases: ["福尚云开发示例"],
  unifiedCreditCode: "91110108MA01FSY01X",
};

/** 使用 ERP 业务时区，避免开发机时区改变合同生效日期。 */
export function todayBusinessDate() {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Shanghai", year: "numeric", month: "2-digit", day: "2-digit",
  }).format(new Date());
}

/** 使用自然日偏移，不把未核实的起始日期补为当天。 */
function offsetDate(today, days) {
  const date = new Date(`${today}T00:00:00Z`);
  date.setUTCDate(date.getUTCDate() + days);
  return date.toISOString().slice(0, 10);
}

export function supplierContract(spec, today) {
  return {
    qualification_type: "contract",
    certificate_no: `DEV-HT-${spec.supplierNo}`,
    issuer: null,
    valid_from: spec.contractState === "unverified" ? null : offsetDate(today, -365),
    valid_to: offsetDate(today, spec.contractState === "expired" ? -1 : 1826),
    attachment_id: null,
    capability_codes: [...spec.capabilityCodes],
  };
}

/** 公司只通过专用接口创建；旧普通主体不得重复新建或静默更换身份。 */
export async function ensureCompanyParty(call, token) {
  const spec = COMPANY_PARTY;
  const page = await call("GET", `/admin/companies?keyword=${encodeURIComponent(spec.legalName)}&page=1&page_size=100`, { token });
  const named = (page.items ?? []).find((row) => row.party_no === spec.partyNo);
  if (named) {
    if (named.status !== "active" || named.legal_name !== spec.legalName || named.unified_credit_code !== spec.unifiedCreditCode) {
      throw new Error("开发公司主体资料与种子不一致，请核对公司主体维护；种子不会覆盖已有资料");
    }
    const aliases = [...new Set([...(named.aliases ?? []), ...spec.aliases])];
    if (aliases.length === (named.aliases ?? []).length) return named;
    return call("PUT", `/admin/companies/${encodeURIComponent(named.id)}`, {
      token, body: { party_no: named.party_no, version: named.version,
        legal_name: named.legal_name, short_name: named.short_name, aliases,
        unified_credit_code: named.unified_credit_code, status: named.status },
    });
  }
  const parties = await call("GET", `/admin/parties?keyword=${encodeURIComponent(spec.partyNo)}&page=1&page_size=100`, { token });
  if ((parties.items ?? []).some((row) => row.party_no === spec.partyNo)) {
    throw new Error("FSY 已存在普通主体或不同公司资料，请先完成公司角色映射；重建开发库时使用 prepare-dev.sh，不得重复创建主体");
  }
  return call("POST", "/admin/companies", {
    token, body: { party_no: spec.partyNo, version: null, legal_name: spec.legalName,
      short_name: spec.shortName, aliases: spec.aliases,
      unified_credit_code: spec.unifiedCreditCode, status: "active" },
  });
}

/** 仅构造完整开发样例，实际创建仍复用供应商根命令。 */
export function supplierCommand(spec, companyPartyId, today = todayBusinessDate()) {
  return {
    idempotency_key: `seed-supplier-${spec.supplierNo}`,
    party_no: spec.partyNo, supplier_no: spec.supplierNo,
    expected_party_version: null, expected_supplier_version: null,
    legal_name: spec.legalName, short_name: spec.shortName,
    unified_credit_code: spec.unifiedCreditCode,
    contact: { contact_name: spec.contactName, mobile: spec.mobile, telephone: null, email: null },
    clear_contact: false,
    address: { address: spec.address, contact_name: spec.contactName }, clear_address: false,
    tax_no: spec.taxNo, clear_tax_profile: false,
    bank_account: { bank_name: spec.bankName, account_number: spec.accountNumber }, clear_bank_account: false,
    settlement_mode: spec.settlementMode, reconciliation_cycle: spec.reconciliationCycle,
    payment_term_snapshot: spec.paymentTerm, business_category: spec.businessCategory,
    invoice_type: spec.invoiceType, invoice_tax_rates: [...spec.invoiceTaxRates],
    signing_entity_party_id: companyPartyId, payment_entity_party_id: companyPartyId,
    capability_codes: [...spec.capabilityCodes], qualifications: [supplierContract(spec, today)],
    rating: { initial_score: spec.score, rating: spec.rating, current_score: spec.score, valid_from: today },
    effective_from: today, change_reason: "主数据初始化：供应商建档",
  };
}

const ratesKey = (rates) => [...rates].map((rate) => {
  const [whole, fraction = ""] = String(rate).split(".");
  const decimals = fraction.replace(/0+$/, "");
  return decimals ? `${whole}.${decimals}` : whole;
}).sort().join(",");

/** 同一供应商可有多个候选税率，每个供给必须回读为本 SKU 指定的单个税率。 */
export function verifyOffering(row, spec, supplierId, skuId, today = todayBusinessDate()) {
  const expectedRate = spec.inputTaxRate ?? (spec.kind === "PHYSICAL" ? "0.13" : "0.06");
  if (row.supplier_id !== supplierId || row.sku_id !== skuId || row.status !== "ACTIVE" ||
      row.availability_status !== "AVAILABLE" || !row.valid_from || row.valid_from > today ||
      (row.valid_to && row.valid_to < today) || ratesKey([row.input_tax_rate ?? ""]) !== ratesKey([expectedRate])) {
    throw new Error(`供给 ${spec.supplierNo}/${spec.skuNo} 的税率、状态或有效期与最新种子不一致，请核对后重新准备开发库`);
  }
  return row;
}

/** 已有种子必须满足当前合同，不能只凭编号宣告成功或覆盖历史商务资料。 */
export async function verifySupplier(call, token, supplier, spec, companyPartyId, today = todayBusinessDate()) {
  const detail = await call("GET", `/admin/suppliers/${encodeURIComponent(supplier.id)}`, { token });
  const profile = detail.current_profile;
  const rates = profile?.invoice_tax_rates ?? (profile?.invoice_tax_rate == null ? [] : [profile.invoice_tax_rate]);
  const contract = (detail.qualifications ?? []).find((row) => row.certificate_no === `DEV-HT-${spec.supplierNo}`);
  const linked = new Set(contract?.capability_ids ?? []);
  const capabilities = (detail.capabilities ?? []).filter((row) => row.status === "active");
  const validDates = contract?.valid_from && contract.valid_from <= today && contract.valid_to >= today;
  const expectedDates = spec.contractState === "unverified" ? contract?.valid_from == null && !!contract?.valid_to
    : spec.contractState === "expired" ? !!contract?.valid_from && contract.valid_to < today : validDates;
  if (!profile || detail.status !== "active" || detail.party_status !== "active" ||
      profile.settlement_mode !== spec.settlementMode || profile.reconciliation_cycle !== spec.reconciliationCycle ||
      profile.payment_term_snapshot !== spec.paymentTerm || ratesKey(rates) !== ratesKey(spec.invoiceTaxRates) ||
      profile.signing_entity_party_id !== companyPartyId || profile.payment_entity_party_id !== companyPartyId ||
      contract?.status !== "active" || !expectedDates ||
      !spec.capabilityCodes.every((code) => capabilities.some((cap) => cap.capability_code === code && linked.has(cap.id)))) {
    throw new Error(`供应商 ${spec.supplierNo} 的公司、结算、税率或合同与最新种子不一致，请核对后重新准备开发库；不会覆盖已有业务资料`);
  }
  return detail;
}

/** 新建后回读验证；重跑复用原供应商身份。 */
export async function ensureSupplier(call, token, spec, companyPartyId) {
  const page = await call("GET", `/admin/suppliers?keyword=${encodeURIComponent(spec.supplierNo)}&page=1&page_size=100`, { token });
  let supplier = (page.items ?? []).find((row) => row.supplier_no === spec.supplierNo);
  if (!supplier) {
    const created = await call("POST", "/admin/supplier-profiles", { token, body: supplierCommand(spec, companyPartyId) });
    supplier = { id: created.supplier_id, supplier_no: created.supplier_no };
  }
  await verifySupplier(call, token, supplier, spec, companyPartyId);
  return supplier;
}

/** 附加周期与阻断样例均为明确标注的开发示例，不使用真实公司别名。 */
export const SUPPLIER_SCENARIOS = [
  ["WEEK", "周结", "weekly", "PERIOD_WEEK_0", "valid"],
  ["QUARTER", "季结", "quarterly", "PERIOD_QUARTER_27", "valid"],
  ["HALF", "半年结", "half_yearly", "PERIOD_HALF_YEAR_15", "valid"],
  ["YEAR", "年结", "yearly", "PERIOD_YEAR_366", "valid"],
  ["UNVERIFIED", "合同未核实", "monthly", "PERIOD_MONTH_15", "unverified"],
  ["EXPIRED", "合同已到期", "monthly", "PERIOD_MONTH_15", "expired"],
].map(([code, label, settlementMode, paymentTerm, contractState], index) => ({
  supplierNo: `SUP-DEV-${code}`, partyNo: `PTY-DEV-${code}`,
  legalName: `开发${label}示例供应商`, shortName: `开发${label}示例`, unifiedCreditCode: null,
  contactName: "开发示例联系人", mobile: `1390000010${index}`, address: "开发环境示例地址",
  bankName: "开发示例银行", accountNumber: `00000000000000010${index}`, taxNo: null,
  settlementMode, reconciliationCycle: settlementMode, paymentTerm, contractState,
  businessCategory: "开发验证样例", invoiceType: "vat_special", invoiceTaxRates: ["0.09", "0.13"],
  capabilityCodes: ["physical"], rating: "A", score: 90,
}));
