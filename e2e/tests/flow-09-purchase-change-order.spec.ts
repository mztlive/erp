/**
 * 流程: [flow-09] 采购变更单（未执行）
 * 文档: docs/erp-phase-1.md §6.5.2；审批合同 §4.3/§4.4；工作台合同第 3 节
 * 账号: xiaoshou（销售提交）→ caigou（采购确认销售单、供给分配、发起并提交采购变更）
 *       → caiwu（财务审批采购单 / 复核采购变更）→ cangchu（仓储确认变更）
 * 文档-代码差异见文件末尾 JSON 报告中的 doc_mismatches。
 *
 * 以代码为准：
 * - 采购变更走统一 DOCUMENT_APPROVAL（仓储确认 → 财务复核），W01 原地通过/驳回
 * - 末节点通过即 on_final_approve 生效；客户端 /effect 关闭
 * - 前端发起变更原因写死为「采购变更」，提交空行时后端复制基准版本行
 */
import path from "node:path"

import { test, expect, type Page } from "@playwright/test"

import { createCustomerViaUi } from "../helpers/customers"
import { openLoggedInWorkspace } from "../helpers/login"
import { ensureDefaultProcurementOwner } from "../helpers/procurement"
import {
    approveCurrentDocument,
    chooseOption,
    expectToast,
    openWorkspaceTask,
    pickCalendarDay,
    selectWorkspaceFamily,
} from "../helpers/ui"

const SAMPLE_CONTRACT_PDF = path.join(
    process.cwd(),
    "fixtures/sample-contract.pdf",
)
const SKU_NAME = "狮峰明前龙井礼盒"
const WAREHOUSE_NAME = "北京通州仓"
const WAREHOUSE_CODE = "BJ-TZ-01"
const VISIBLE = { timeout: 20_000 } as const

test.describe.configure({ mode: "serial" })
test.setTimeout(8 * 60 * 1000)

function todayIso(): string {
    const date = new Date()
    const pad = (value: number) => String(value).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

async function fillEmptyDatePickers(page: Page) {
    const iso = todayIso()
    const empty = page.getByRole("button", { name: "选择日期" })
    const total = await empty.count()
    for (let index = 0; index < total; index += 1) {
        const remaining = page.getByRole("button", { name: "选择日期" })
        if ((await remaining.count()) === 0) break
        await pickCalendarDay(page, remaining.first(), iso)
    }
}

async function approveWorkspaceTask(
    page: Page,
    taskName: RegExp,
    currentNode?: string | RegExp,
) {
    await openWorkspaceTask(page, taskName, undefined, "approval")
    if (currentNode) {
        await expect(page.getByText(currentNode).first()).toBeVisible(VISIBLE)
    }
    await expect(page.getByRole("button", { name: /^(通过|同意审批)$/ })).toBeVisible(
        VISIBLE,
    )
    const decided = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().endsWith("/admin/approval-decisions"),
        { timeout: 40_000 },
    )
    await approveCurrentDocument(page)
    expect((await decided).ok()).toBeTruthy()
    await expect(page.getByRole("button", { name: taskName })).toHaveCount(0, VISIBLE)
}

test("[flow-09] 采购单未入库未付款时走采购变更单并生效", async ({
    browser,
}) => {
    const stamp = Date.now().toString(10)
    const creditCode = `91${stamp}FLOW09XX`.replace(/[^0-9A-Za-z]/g, "").slice(0, 18).padEnd(18, "0")
    const legalName = `华润置地福利测试${stamp.slice(-8)}`
    const contractNo = `HT-FLOW09-${stamp.slice(-8)}`

    // 1. 主数据：若无采购责任规则则由 admin 补默认调度人 caigou（否则销售提交被拦）
    {
        const admin = await openLoggedInWorkspace(browser, "admin")
        try {
            await ensureDefaultProcurementOwner(admin.page)
        } finally {
            await admin.context.close()
        }
    }

    // 2. 销售：客户 + 合同 PDF + 实物销售单提交
    const sales = await openLoggedInWorkspace(browser, "xiaoshou")
    try {
        await createCustomerViaUi(sales.page, {
            legalName,
            shortName: `测试客户${stamp.slice(-6)}`,
            creditCode,
            paymentTermLabel: "货到 15 天",
            contact: { name: "李测", phone: "13800138001" },
            address: "北京市朝阳区测试路 1 号",
        })

        await sales.page.goto("/sales/contracts")
        await expect(sales.page.getByRole("heading", { name: "合同" })).toBeVisible(
            VISIBLE,
        )
        await sales.page.getByLabel("页面操作").getByRole("button", { name: "上传合同 PDF" }).click()
        await expect(
            sales.page.getByRole("heading", { name: "上传合同 PDF" }),
        ).toBeVisible(VISIBLE)
        await sales.page
            .locator("#card-contracts-upload-pdf-input")
            .setInputFiles(SAMPLE_CONTRACT_PDF)
        await sales.page.locator("#card-contracts-upload-contract-no").fill(contractNo)
        await chooseOption(
            sales.page,
            sales.page.locator("#card-contracts-upload-customer"),
            new RegExp(legalName),
            legalName,
        )
        await expect(
            sales.page.locator("#card-contracts-upload-settlement-party"),
        ).not.toHaveValue("", { timeout: 20_000 })
        await sales.page.locator("#card-contracts-upload-submit").click()
        await expectToast(sales.page, "合同 PDF 已归档")

        await sales.page.goto("/sales/orders?mode=create")
        await expect(
            sales.page
                .getByRole("heading", { name: "新建销售单" })
                .or(sales.page.getByRole("heading", { name: "业务信息" })),
        ).toBeVisible(VISIBLE)
        await chooseOption(
            sales.page,
            sales.page.locator("#sales-orders-create-contract"),
            new RegExp(contractNo),
            contractNo,
        )
        await expect(sales.page.getByText(legalName, { exact: true }).first()).toBeVisible(VISIBLE)
        await chooseOption(
            sales.page,
            sales.page.locator("#sales-orders-create-header-welfare-scene"),
            "年节礼包",
        )
        await sales.page.locator("#sales-orders-create-line-items-add").click()
        const skuDialog = sales.page.getByRole("dialog", { name: "添加商品" })
        await expect(skuDialog).toBeVisible(VISIBLE)
        await skuDialog
            .getByPlaceholder("搜索 SKU、商品名称、编号或规格")
            .fill(SKU_NAME)
        await skuDialog
            .getByPlaceholder("搜索 SKU、商品名称、编号或规格")
            .press("Enter")
        const skuRow = skuDialog.getByRole("row", { name: new RegExp(SKU_NAME) })
        await expect(skuRow).toBeVisible(VISIBLE)
        await skuRow.getByRole("checkbox").check()
        await skuDialog.getByRole("button", { name: /加入所选/ }).click()
        await expect(skuDialog).toBeHidden(VISIBLE)
        await expect(sales.page.getByRole("button", { name: new RegExp(`更换销售项目 ${SKU_NAME}`) })).toBeVisible(VISIBLE)
        await sales.page
            .locator('input[id^="sales-orders-create-line-"][id$="-quantity"]')
            .fill("2")
        await sales.page.locator("#sales-orders-create-batch-due-date-open").click()
        await pickCalendarDay(
            sales.page,
            sales.page.locator("#sales-orders-create-batch-due-date"),
            todayIso(),
        )
        await sales.page
            .locator("#sales-orders-create-batch-due-date-apply")
            .click()
        await expectToast(sales.page, "已批量设置交期")
        await expect(
            sales.page.locator('[data-testid^="sales-line-procurement-owner-"]'),
        ).not.toContainText("暂未确定采购负责人", VISIBLE)
        await sales.page.getByTestId("sales-order-submit").click()
        await expect(
            sales.page.getByRole("heading", { name: "提交销售单" }),
        ).toBeVisible(VISIBLE)
        await sales.page.locator("#sales-orders-submit-confirm-confirm").click()
        await expect(sales.page).toHaveURL(/\/sales\/orders\/[^/?#]+/, VISIBLE)
        await expect(
            sales.page.locator("header").getByText("审批中"),
        ).toBeVisible(VISIBLE)
    } finally {
        await sales.context.close()
    }

    // 3. 采购确认销售单（采购确认节点不选供给）
    {
        const procurement = await openLoggedInWorkspace(browser, "caigou")
        try {
            await approveWorkspaceTask(
                procurement.page,
                /销售单审批/,
                "采购确认",
            )
        } finally {
            await procurement.context.close()
        }
    }

    // 4. 供给分配：库存为空必须生成采购单并立即提交审批
    {
        const procurement = await openLoggedInWorkspace(browser, "caigou")
        try {
            await openWorkspaceTask(procurement.page, /待供给分配/, undefined, "procurement")
            await expect(
                procurement.page.getByRole("region", { name: "当前供给分配任务" }),
            ).toBeVisible(VISIBLE)
            await expect(
                procurement.page.getByText("销售明细与供给方案"),
            ).toBeVisible(VISIBLE)
            await procurement.page
                .getByTestId("purchase-create-match-best")
                .click()
            await expectToast(procurement.page, /已重新分配供给|没有可匹配的供给方案/)
            await expect(
                procurement.page.getByText("销售明细与供给方案"),
            ).toBeVisible(VISIBLE)
            const expandSourcing = procurement.page.getByRole("button", { name: "调整方案" }).first()
            if (await expandSourcing.isVisible().catch(() => false)) {
                await expandSourcing.click()
            }
            const warehouseInput = procurement.page.getByRole("combobox", { name: "仓库", exact: true })
            if ((await warehouseInput.count()) > 0) {
                // 仓库下拉按仓库代码精确筛选，填代码后按名称选择选项。
                await chooseOption(
                    procurement.page,
                    warehouseInput,
                    new RegExp(`${WAREHOUSE_CODE}|${WAREHOUSE_NAME}`),
                    WAREHOUSE_CODE,
                )
            }
            await fillEmptyDatePickers(procurement.page)
            await procurement.page.getByTestId("purchase-create-preview").click()
            await expect(
                procurement.page.getByRole("heading", { name: "预览供给分配" }),
            ).toBeVisible(VISIBLE)
            await expect(
                procurement.page.getByText(/本次不占用现有库存|将为供给缺口创建|张采购单提交审批/),
            ).toBeVisible(VISIBLE)
            await expect(
                procurement.page.getByText("无需创建采购单"),
            ).toHaveCount(0)
            await procurement.page
                .locator("#procurement-orders-create-preview-confirm")
                .click()
            await expectToast(procurement.page, /供给分配已完成|本次供给分配已保存/)
            await expect(
                procurement.page.locator("[data-slot=toast-description]").filter({
                    hasText: /无需采购/,
                }),
            ).toHaveCount(0)
        } finally {
            await procurement.context.close()
        }
    }

    // 5. 财务审批采购单 → 采购单生效、形成应付；本流程不付款、不入库
    {
        const finance = await openLoggedInWorkspace(browser, "caiwu")
        try {
            await approveWorkspaceTask(
                finance.page,
                /采购单审批/,
                "财务总监审批",
            )
        } finally {
            await finance.context.close()
        }
    }

    const procurement = await openLoggedInWorkspace(browser, "caigou")
    let purchaseHref = ""
    try {
        await procurement.page.goto("/procurement/orders")
        await expect(
            procurement.page.getByRole("heading", { name: "采购单", exact: true }),
        ).toBeVisible(VISIBLE)
        const openPo = procurement.page.getByRole("button", {
            name: /打开采购单/,
        })
        await expect(openPo).toBeVisible(VISIBLE)
        await openPo.click()
        await expect(procurement.page).toHaveURL(
            /\/procurement\/orders\/[^/?#]+/,
            VISIBLE,
        )
        purchaseHref = procurement.page.url()
        await expect(procurement.page.locator("header").getByText("已生效", { exact: true })).toBeVisible(VISIBLE)
        await expect(
            procurement.page.locator('[aria-label="采购单摘要"]').getByText("未付"),
        ).toBeVisible(VISIBLE)
        await expect(
            procurement.page.locator('[aria-label="采购单摘要"]').getByText("未开始"),
        ).toBeVisible(VISIBLE)

        // 负向：未执行前不得把履约/付款当成本流程
        await expect(
            procurement.page.getByRole("button", { name: /确认入库|确认发货|确认入账/ }),
        ).toHaveCount(0)

        // 6. 发起采购变更（未入库未付款，走变更单而非纠正单）
        await procurement.page.locator("#procurement-orders-detail-change").click()
        await expect(
            procurement.page.getByRole("heading", { name: "发起采购变更" }),
        ).toBeVisible(VISIBLE)
        await procurement.page.getByRole("button", { name: "创建工作副本" }).click()
        await expect(
            procurement.page.getByRole("heading", {
                name: "已创建采购变更工作副本",
            }),
        ).toBeVisible(VISIBLE)
        await expect(procurement.page).toHaveURL(/section=changes/, VISIBLE)
        await expect(procurement.page.getByRole("listitem").filter({ hasText: "采购变更" }).getByText("草稿", { exact: true })).toBeVisible(VISIBLE)

        // 进行中改单时不得再开第二张变更单
        await expect(
            procurement.page.locator("#procurement-orders-detail-change"),
        ).toHaveCount(0)
        const disabledChange = procurement.page.locator(
            `[id^="procurement-orders-detail-changes-disabled-"]`,
        )
        if ((await disabledChange.count()) > 0) {
            await expect(disabledChange).toBeDisabled()
        }

        // 7. 提交改单：代码无数量/成本编辑面，后端空行则复制基准版本
        await procurement.page.getByRole("button", { name: "提交改单" }).click()
        await expect(
            procurement.page.getByRole("heading", { name: "确认提交改单" }),
        ).toBeVisible(VISIBLE)
        await procurement.page.getByRole("button", { name: "确认提交" }).click()
        await expect(
            procurement.page.getByRole("heading", { name: "改单已提交审批" }),
        ).toBeVisible(VISIBLE)
        await expect(procurement.page.getByRole("listitem").filter({ hasText: "采购变更" }).getByText("审批中", { exact: true })).toBeVisible(VISIBLE)
        // 提交后原采购版本仍有效
        await expect(procurement.page.locator("header").getByText("已生效", { exact: true })).toBeVisible(VISIBLE)
    } finally {
        await procurement.context.close()
    }

    // 8. 仓储确认库存发货影响（统一审批第一节点，W01 原地处理）
    {
        const warehouse = await openLoggedInWorkspace(browser, "cangchu")
        try {
            await warehouse.page.goto("/workspace")
            await expect(
                warehouse.page.getByRole("heading", { name: "我的工作台" }),
            ).toBeVisible(VISIBLE)
            const fulfillmentTask = warehouse.page.getByRole("button", {
                name: /履约处理/,
            })
            if ((await fulfillmentTask.count()) > 0) {
                await selectWorkspaceFamily(warehouse.page, "approval")
                await expect(fulfillmentTask).toHaveCount(0)
            }
            await approveWorkspaceTask(
                warehouse.page,
                /采购变更单审批/,
                "仓储确认库存发货影响",
            )
        } finally {
            await warehouse.context.close()
        }
    }

    // 9. 财务复核金额与应付；末节点通过即生效
    {
        const finance = await openLoggedInWorkspace(browser, "caiwu")
        try {
            await approveWorkspaceTask(
                finance.page,
                /采购变更单审批/,
                "财务复核金额与应付",
            )
        } finally {
            await finance.context.close()
        }
    }

    // 10. 断言：变更已生效，采购单/应付按变更更新；仍未付款未履约
    {
        const procurement = await openLoggedInWorkspace(browser, "caigou")
        try {
            await procurement.page.goto(purchaseHref)
            await expect(procurement.page.locator("header").getByText("已生效", { exact: true })).toBeVisible(VISIBLE)
            await expect(procurement.page.locator("header").getByText("已生效", { exact: true })).toBeVisible(VISIBLE)
            await procurement.page
                .getByRole("tab", { name: "变更" })
                .click()
            await expect(procurement.page.getByRole("listitem").filter({ hasText: "采购变更" }).getByText("已生效", { exact: true })).toBeVisible(VISIBLE)
            await expect(
                procurement.page.getByRole("button", { name: "提交改单" }),
            ).toHaveCount(0)
            await procurement.page.getByRole("tab", { name: "票款" }).click()
            await expect(procurement.page.getByText("应付未结")).toBeVisible(
                VISIBLE,
            )
            await expect(
                procurement.page.getByText("尚未形成应付（需审批通过）。"),
            ).toHaveCount(0)
            await expect(procurement.page.getByText("已付并核销")).toBeVisible(
                VISIBLE,
            )
            await procurement.page.getByRole("tab", { name: "概览" }).click()
            await expect(procurement.page.getByText("未付")).toBeVisible(VISIBLE)
            await expect(procurement.page.getByText("未开始")).toBeVisible(VISIBLE)
            await expect(
                procurement.page.getByRole("button", { name: /确认入库|确认发货/ }),
            ).toHaveCount(0)
        } finally {
            await procurement.context.close()
        }
    }

    // 仓储工作台：未执行本流程不得把履约任务当变更完成
    {
        const warehouse = await openLoggedInWorkspace(browser, "cangchu")
        try {
            await warehouse.page.goto("/workspace")
            await expect(
                warehouse.page.getByRole("heading", { name: "我的工作台" }),
            ).toBeVisible(VISIBLE)
            await selectWorkspaceFamily(warehouse.page, "approval")
            await expect(
                warehouse.page
                    .getByRole("list", { name: "待办列表" })
                    .getByRole("button", { name: /采购变更单审批/ }),
            ).toHaveCount(0)
        } finally {
            await warehouse.context.close()
        }
    }

})
