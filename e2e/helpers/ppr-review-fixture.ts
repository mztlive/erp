import { randomUUID } from "node:crypto"

import { expect, type Page } from "./test"
import { API_BASE, apiGet } from "./api"
import { analyticsDate, uploadAnalyticsEvidence, type AnalyticsOrder } from "./financial-analytics"
import { approveCurrentDocument, openWorkspaceTask } from "./ui"

export type ReviewCompany = {
    id: string; version: number; party_no: string; legal_name: string; short_name: string | null;
    aliases: string[]; unified_credit_code: string | null; status: "active" | "disabled";
}

export type ReviewTask = {
    id: string; task_version: string; owner_user_id: string | null; status: string;
    business_object_id: string; business_object_type: string; allowed_actions: string[];
}

export async function reviewCommand<T>(page: Page, token: string, method: "POST" | "PUT", path: string, data: unknown): Promise<T> {
    const response = await page.request.fetch(`${API_BASE}${path}`, {
        method, headers: { Authorization: `Bearer ${token}` }, data, timeout: 60_000,
    })
    expect(response.ok(), await response.text()).toBe(true)
    const envelope = await response.json() as { success: boolean; errorMessage?: string; data: T }
    expect(envelope.success, envelope.errorMessage).toBe(true)
    return envelope.data
}

/** 本规格独有的公司和客户建档；不修改固定公司、供应商、岗位或商档。 */
export async function createReviewFixture(page: Page, adminToken: string, salesToken: string, suffix: string) {
    const company = await reviewCommand<ReviewCompany>(page, adminToken, "POST", "/admin/companies", {
        party_no: `E2E-PPR-REVIEW-${suffix}`, legal_name: `E2E 付款主体原名 ${suffix}`,
        short_name: null, aliases: [], unified_credit_code: null, status: "active",
    })
    const customer = await reviewCommand<{ customer_id: string }>(page, salesToken, "POST", "/admin/customer-profiles", {
        idempotency_key: randomUUID(), legal_name: `E2E 付款复核客户 ${suffix}`, unified_credit_code: null,
        default_payment_term_id: "POSTPAY_NET15", effective_from: analyticsDate(), change_reason: "E2E 付款申请 review 回归",
    })
    const evidenceId = await uploadAnalyticsEvidence(salesToken)
    const accounts = await apiGet<Array<{ id: string; account: string }>>(adminToken, "/admin/admins")
    const sales = accounts.find((item) => item.account === "xiaoshou")
    expect(sales).toBeTruthy()
    return { company, customerId: customer.customer_id, evidenceId, salesUserId: sales!.id }
}

/** 正式销售命令冻结调用当时的我方公司名称，采购继续继承精确版本。 */
export async function createReviewOrder(
    page: Page, procurementToken: string, salesToken: string,
    fixture: { company: ReviewCompany; customerId: string; evidenceId: string; salesUserId: string },
    suffix: string, kind: "service" | "goods",
): Promise<AnalyticsOrder> {
    const name = kind === "service" ? "家电上门安装（标准台）" : "狮峰明前龙井礼盒 250g"
    const keyword = kind === "service" ? "SVC-INSTALL-01" : "TEA-SF-LJ-250"
    const pool = await apiGet<{ items: Array<{ sku_id: string; sku_revision_id: string; sku_no: string }> }>(salesToken,
        "/admin/sellable-skus", { q: keyword, page_size: 100 })
    const sku = pool.items.find((item) => item.sku_no === keyword)
    expect(sku, `固定种子需要销售 SKU ${keyword}`).toBeTruthy()
    const created = await reviewCommand<AnalyticsOrder>(page, salesToken, "POST", "/admin/sales-orders", {
        order_no: `XS-PPR-REVIEW-${suffix}`, business_type: "GOODS_SERVICE", contract_id: null,
        customer_id: fixture.customerId, idempotency_key: randomUUID(), intent: "SUBMIT",
        evidence_file_asset_ids: [fixture.evidenceId],
        draft: {
            editor_user_id: fixture.salesUserId, requested_contract_revision_id: null,
            no_contract_terms: {
                settlement_party_id: fixture.company.id, payment_term_code: "POSTPAY_NET15", payment_term_name: "货到 15 天",
                invoice_type: "增值税专用发票", tax_point: "0",
            },
            lines: [{
                line_no: 1, line_type: "GOODS_SERVICE", sales_tax_rate: "0", item_name_snapshot: name,
                spec_snapshot: kind === "service" ? "标准台" : "250g/盒", unit_snapshot: kind === "service" ? "次" : "盒",
                goods: {
                    sku_id: sku!.sku_id, sku_revision_id: sku!.sku_revision_id,
                    service_region: kind === "service" ? "北京" : null,
                    fulfillment_due_at: Math.floor(Date.now() / 1000) + 30 * 86400,
                    quantity: kind === "service" ? "2" : "1", base_unit_code: kind === "service" ? "CI" : "HE",
                    unit_price_gross: kind === "service" ? "100.00" : "1000.00", pricing_mode: "MANUAL",
                },
            }],
        },
    })
    await openWorkspaceTask(page, "销售单审批", created.order_no, "approval")
    await approveCurrentDocument(page)
    const effective = await apiGet<AnalyticsOrder>(procurementToken, `/admin/sales-orders/${created.id}`)
    expect(effective.commercial_status).toBe("EFFECTIVE")
    return effective
}

/** 只改派本规格新建的正式任务；既有默认责任配置保持原事实。 */
export async function assignReviewTask(page: Page, token: string, taskId: string, account: string): Promise<ReviewTask> {
    const task = await apiGet<ReviewTask>(token, `/admin/work-items/${taskId}`)
    const candidates = await apiGet<Array<{ user_id: string; account: string }>>(token, `/admin/work-items/${taskId}/reassign-candidates`)
    const target = candidates.find((candidate) => candidate.account === account)
    expect(target, `当前任务的完整业务资格候选必须包含 ${account}`).toBeTruthy()
    const assigned = await reviewCommand<ReviewTask>(page, token, "POST", `/admin/work-items/${taskId}/reassign`, {
        expected_task_version: task.task_version, target_user_id: target!.user_id,
        reason: "E2E 同一合格账号验证付款后的履约缓存更新", idempotency_key: randomUUID(),
    })
    expect(assigned).toMatchObject({ id: taskId, owner_user_id: target!.user_id, status: "OPEN" })
    const qualified = await apiGet<ReviewTask>(token, `/admin/work-items/${taskId}`)
    expect(qualified.allowed_actions).toContain("PROCESS")
    return qualified
}
