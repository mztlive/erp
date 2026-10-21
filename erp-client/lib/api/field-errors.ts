/** 接口字段与页面字段的展示约定；未知字段不把内部名称直接交给用户。 */
const labels: Record<string, string> = {
    legal_name: "法定名称",
    short_name: "简称",
    name: "名称",
    customer_no: "客户编号",
    customer_id: "客户",
    supplier_id: "供应商",
    contract_id: "合同",
    warehouse_id: "仓库",
    contact: "联系人",
    contacts: "联系人",
    bank_account: "银行账户",
    qualifications: "资质",
    certificate_no: "证书编号",
    addresses: "地址",
    bank_accounts: "银行账户",
    lines: "明细",
    account_name: "户名",
    bank_name: "银行名称",
    bank_branch_name: "支行名称",
    account_number: "银行账号",
    contact_name: "联系人姓名",
    mobile: "手机号",
    phone: "电话",
    email: "邮箱",
    address: "地址",
    legal_representative: "法定代表人",
    unified_credit_code: "统一社会信用代码",
    reason: "原因",
    change_reason: "变更原因",
    remark: "备注",
    business_remark: "业务备注",
    quantity: "数量",
    amount: "金额",
    price: "价格",
    tax_rate: "税率",
    unit_price: "单价",
    product_name: "商品名称",
    product_id: "商品",
    sku_id: "商品规格",
    sku_no: "SKU 编号",
    effective_from: "生效日期",
    due_date: "到期日期",
    page: "页码",
    page_size: "每页数量",
    expected_version: "资料版本",
    expected_working_copy_version: "草稿版本",
    expected_party_version: "主体资料版本",
    expected_customer_version: "客户资料版本",
    idempotency_key: "本次操作标识",
    request_id: "本次操作标识",
    status: "状态",
    category_id: "分类",
    brand_id: "品牌",
    base_unit_id: "基础单位",
    code: "编号",
}

export function fieldErrorLabel(path: string): string {
    return path
        .split(".")
        .map((part) => {
            const match = /^(\w+)(?:\[(\d+)\])?$/.exec(part)
            if (!match) return "相关字段"
            const label = labels[match[1]] ?? "相关字段"
            return match[2] === undefined
                ? label
                : `${label}第 ${BigInt(match[2]) + BigInt(1)} 行`
        })
        .join(" · ")
}

export function fieldErrorMessages(
    fields?: Readonly<Record<string, string>>,
): string[] {
    return Object.entries(fields ?? {}).map(
        ([path, message]) => `${fieldErrorLabel(path)}：${message}`,
    )
}

/** 只负责命名转换；不同业务字段的特殊映射由调用方明确提供。 */
export function formFieldPath(
    path: string,
    aliases: Readonly<Record<string, string>> = {},
): string {
    if (aliases[path]) return aliases[path]
    return path
        .split(".")
        .map((part) => {
            const match = /^(\w+)(\[\d+\])?$/.exec(part)
            if (!match) return part
            return (
                (aliases[match[1]] ??
                    match[1].replace(/_([a-z])/g, (_, letter: string) =>
                        letter.toUpperCase(),
                    )) + (match[2] ?? "")
            )
        })
        .join(".")
}
