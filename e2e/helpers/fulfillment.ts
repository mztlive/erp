import { workPage } from "./workspace-page"
import { basename } from "node:path"
import { fileURLToPath } from "node:url"

import { expect, type Page } from "@playwright/test"

import { toAutomationIdSegment } from "../../erp-client/lib/automation-id"
import { apiGet } from "./api"
import { chooseOption, openWorkspaceTaskRow, selectWorkspaceFamily, UI_TIMEOUT } from "./ui"

type FulfillmentTaskSource = {
    salesOrderId: string
    purchaseOrderId?: string
    operation: "RECEIPT" | "SUPPLIER_DIRECT" | "WAREHOUSE_SHIP" | "ELECTRONIC" | "SERVICE"
}

export async function openFulfillmentTaskForSource(page: Page, source: FulfillmentTaskSource) {
    const token = await page.evaluate(() => localStorage.getItem("erp.token"))
    expect(token, "履约任务应由当前登录账号读取").toBeTruthy()
    const queue = await apiGet<{
        total: number
        items: Array<{ work_item_id: string; sales_order_id: string; purchase_order_id?: string; operation_type: string }>
    }>(token!, "/admin/work-items/fulfillment-queue", {
        sales_order_id: source.salesOrderId,
        purchase_order_id: source.purchaseOrderId,
        operation_types: source.operation,
        timezone: "Asia/Shanghai",
        page: 1,
        page_size: 10,
    })
    expect(queue.total, "应有唯一匹配来源和类型的履约任务").toBe(1)
    expect(queue.items).toHaveLength(1)
    const task = queue.items[0]!
    expect(task).toMatchObject({ sales_order_id: source.salesOrderId, operation_type: source.operation })
    if (source.purchaseOrderId) expect(task.purchase_order_id).toBe(source.purchaseOrderId)
    expect(task.work_item_id).toBeTruthy()

    await page.goto("/workspace/tasks")
    await selectWorkspaceFamily(page, "fulfillment")
    await openWorkspaceTaskRow(page, task.work_item_id)
}

const ACCEPTANCE_EVIDENCE = fileURLToPath(
    new URL("../fixtures/sample-contract.pdf", import.meta.url),
)

/** 在当前签收登记弹窗选择真实 PDF/图片，并确认文件写入表单。 */
export async function uploadAcceptanceEvidence(
    page: Page,
    fixturePath = ACCEPTANCE_EVIDENCE,
): Promise<void> {
    const dialog = workPage(page).getByRole("dialog", {
        name: "登记客户验收",
        exact: true,
    })
    await expect(dialog).toBeVisible({ timeout: UI_TIMEOUT })
    const input = dialog.locator("#sales-orders-acceptance-evidence-input")
    await expect(input).toBeEnabled({ timeout: UI_TIMEOUT })
    await input.setInputFiles(fixturePath)
    // FileUpload 清空原生 input，以允许重新选同一文件；文件名回显证明 Form 状态已写入。
    await expect(
        dialog.getByText(basename(fixturePath), { exact: true }),
    ).toBeVisible({
        timeout: UI_TIMEOUT,
    })
}

export async function fillFirstDeliveryParcel(page: Page, trackingNo: string, carrier: string) {
    const form = workPage(page).getByRole("region", { name: "按地址登记发货包裹", exact: true })
    await expect(form).toBeVisible({ timeout: UI_TIMEOUT })
    const tracking = form.getByRole("textbox", { name: "包裹1物流单号", exact: true })
    if (!(await tracking.isVisible())) {
        await form.getByRole("button", { name: "登记首个包裹物流", exact: true }).click()
    }
    await tracking.fill(trackingNo)
    const prefix = (await tracking.getAttribute("id"))?.replace(/-tracking$/, "")
    expect(prefix).toBeTruthy()
    await chooseOption(page, form.locator(`#${prefix}-carrier`), carrier)
    return form
}

export type DeliveryTrackingKind = "ship" | "direct"

export type DeliveryTrackingEntryInput = {
    trackingNo: string
    carrier?: string
} & (
    | { salesOrderLineId: string; lineIndex?: never }
    | { salesOrderLineId?: never; lineIndex: number }
)

function deliveryTrackingInputs(page: Page, kind: DeliveryTrackingKind) {
    const region = workPage(page).locator(
        `[aria-label="${kind === "ship" ? "公司仓发表单" : "供应商直发表单"}"]`,
    )
    return {
        region,
        inputs: region.locator(
            `textarea[id^="fulfillment-operations-${kind}-form-line-"][id$="-tracking-no"]`,
        ),
    }
}

/**
 * 在明确的销售明细下添加包裹，返回接收输入的原生 id。
 * 多明细断言使用真实 salesOrderLineId；lineIndex 只选择本次实际发货表单中的行。
 */
export async function addDeliveryTrackingEntry(
    page: Page,
    input: DeliveryTrackingEntryInput & { kind: DeliveryTrackingKind },
): Promise<string> {
    const { region, inputs } = deliveryTrackingInputs(page, input.kind)
    await expect(region).toBeVisible({ timeout: UI_TIMEOUT })
    if (
        input.lineIndex !== undefined &&
        (!Number.isInteger(input.lineIndex) || input.lineIndex < 0)
    ) {
        throw new Error("物流明细行序号必须为非负整数")
    }
    const tracking = input.salesOrderLineId
        ? region.locator(
              `#fulfillment-operations-${input.kind}-form-line-${toAutomationIdSegment(input.salesOrderLineId)}-tracking-no`,
          )
        : inputs.nth(input.lineIndex!)
    await expect(tracking).toBeVisible({ timeout: UI_TIMEOUT })
    const inputId = await tracking.getAttribute("id")
    if (!inputId) throw new Error("物流号输入框缺少原生 id")
    const prefix = inputId.slice(0, -"-tracking-no".length)
    await tracking.fill(input.trackingNo)
    const carrier = region.locator(`#${prefix}-carrier`)
    const currentCarrier = await carrier.inputValue()
    if (currentCarrier && currentCarrier !== (input.carrier ?? "")) {
        // 添加包裹后表单保留承运方。先清除当前选择，使再次搜索不沿用原选项过滤。
        await region.locator(`#${prefix}-carrier-clear`).click()
        await expect(carrier).toHaveValue("", { timeout: UI_TIMEOUT })
    }
    if (input.carrier) {
        await chooseOption(page, carrier, input.carrier)
        await expect(carrier).toHaveValue(input.carrier, {
            timeout: UI_TIMEOUT,
        })
    }
    await region.locator(`#${prefix}-add`).click()
    // 仅填写输入不会进入提交 payload，添加后输入清空才代表本明细包裹已登记。
    await expect(tracking).toHaveValue("", { timeout: UI_TIMEOUT })
    return inputId
}

/** 对每个给定的真实销售明细逐一添加包裹，不推断或合并其他明细。 */
export async function fillDeliveryTrackingEntries(
    page: Page,
    input: {
        kind: DeliveryTrackingKind
        entries: readonly {
            salesOrderLineId: string
            trackingNo: string
            carrier?: string
        }[]
    },
): Promise<void> {
    for (const entry of input.entries) {
        await addDeliveryTrackingEntry(page, { kind: input.kind, ...entry })
    }
}

/** 同一包裹承载本次所有发货明细时，在每条实际行下分别登记其关联。 */
export async function fillAllDeliveryLineTracking(
    page: Page,
    input: { kind: DeliveryTrackingKind; trackingNo: string; carrier?: string },
): Promise<void> {
    const { region, inputs } = deliveryTrackingInputs(page, input.kind)
    await expect(region).toBeVisible({ timeout: UI_TIMEOUT })
    await expect(inputs.first()).toBeVisible({ timeout: UI_TIMEOUT })
    const count = await inputs.count()
    for (let lineIndex = 0; lineIndex < count; lineIndex += 1) {
        await addDeliveryTrackingEntry(page, { ...input, lineIndex })
    }
}
