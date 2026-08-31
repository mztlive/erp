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

import {
    test,
    expect,
    type Browser,
    type Locator,
    type Page,
} from "@playwright/test"

import { ACCOUNTS } from "../helpers/accounts"
import { loginViaUi, newLoggedInContext } from "../helpers/login"

const SAMPLE_CONTRACT_PDF = path.join(
    process.cwd(),
    "fixtures/sample-contract.pdf",
)
const SKU_NAME = "狮峰明前龙井礼盒"
const WAREHOUSE_NAME = "北京通州仓"
const VISIBLE = { timeout: 20_000 } as const

type LoginName = "xiaoshou" | "caigou" | "cangchu" | "caiwu" | "admin"

test.describe.configure({ mode: "serial" })
test.setTimeout(8 * 60 * 1000)

function helperAccount(
    loginName: LoginName,
): Parameters<typeof loginViaUi>[1] {
    const catalog = ACCOUNTS as Record<string, unknown>
    if (loginName in catalog) {
        return loginName as Parameters<typeof loginViaUi>[1]
    }
    for (const [key, value] of Object.entries(catalog)) {
        if (value === loginName) {
            return key as Parameters<typeof loginViaUi>[1]
        }
        if (value && typeof value === "object") {
            const rec = value as {
                account?: string
                username?: string
                login?: string
            }
            if (
                rec.account === loginName ||
                rec.username === loginName ||
                rec.login === loginName
            ) {
                return key as Parameters<typeof loginViaUi>[1]
            }
        }
    }
    return loginName as Parameters<typeof loginViaUi>[1]
}

async function openSession(browser: Browser, loginName: LoginName) {
    const account = helperAccount(loginName)
    const session = await newLoggedInContext(browser, account)
    const page = session.page
    const context = session.context
    if (!page || !context) {
        throw new Error("newLoggedInContext 必须返回 { page, context }")
    }
    if (page.url().includes("/login")) {
        await loginViaUi(page, account)
    }
    await expect(page.getByRole("button", { name: "登录" })).toHaveCount(
        0,
        VISIBLE,
    )
    return { page, context, account }
}

async function expectToast(page: Page, title: string) {
    await expect(
        page.locator("[data-slot=toast-title]").filter({ hasText: title }),
    ).toBeVisible(VISIBLE)
}

async function chooseComboboxOption(
    page: Page,
    input: Locator,
    query: string,
    option: string | RegExp,
) {
    await input.click()
    await input.fill(query)
    await expect(page.getByRole("option", { name: option }).first()).toBeVisible(
        VISIBLE,
    )
    await page.getByRole("option", { name: option }).first().click()
}

async function pickVisibleCalendarDay(page: Page) {
    const popover = page.locator("[data-slot=popover-content]").last()
    await expect(popover).toBeVisible(VISIBLE)
    const day = String(Math.min(28, Math.max(1, new Date().getDate())))
    const cell = popover
        .locator("button[data-day]:not([disabled])")
        .filter({ hasText: new RegExp(`^${day}$`) })
        .first()
    await expect(cell).toBeVisible(VISIBLE)
    await cell.click()
}

async function pickDateById(page: Page, id: string) {
    await page.locator(`#${id}`).click()
    await pickVisibleCalendarDay(page)
}

async function fillEmptyDatePickers(page: Page, scope?: Page) {
    const root = scope ?? page
    const empty = root.getByRole("button", { name: "选择日期" })
    const total = await empty.count()
    for (let index = 0; index < total; index += 1) {
        const remaining = root.getByRole("button", { name: "选择日期" })
        if ((await remaining.count()) === 0) break
        await remaining.first().click()
        await pickVisibleCalendarDay(page)
    }
}

async function approveWorkspaceTask(
    page: Page,
    taskName: RegExp,
    currentNode?: string | RegExp,
) {
    await page.goto("/workspace")
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible(
        VISIBLE,
    )
    await page.locator("#workspace-family-nav-approval").click()
    const list = page.getByRole("list", { name: "待办列表" })
    const task = list.getByRole("button", { name: taskName })
    await expect(task).toBeVisible(VISIBLE)
    await task.click()
    const pane = page.getByRole("region", { name: "当前任务" })
    await expect(pane).toBeVisible(VISIBLE)
    if (currentNode) {
        await expect(pane.getByText(currentNode)).toBeVisible(VISIBLE)
    }
    await expect(page.getByRole("button", { name: "通过" })).toBeVisible(VISIBLE)
    await page.getByRole("button", { name: "通过" }).click()
    await expect(page.getByRole("heading", { name: "确认通过" })).toBeVisible(
        VISIBLE,
    )
    await page.getByRole("button", { name: "确认通过" }).click()
    await expect(task).toHaveCount(0, VISIBLE)
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
        const admin = await openSession(browser, "admin")
        try {
            await admin.page.goto("/master-data/procurement-responsibilities")
            await expect(
                admin.page.getByRole("heading", { name: "采购责任规则" }),
            ).toBeVisible(VISIBLE)
            const hasDispatcher = await admin.page
                .getByText("默认调度人")
                .count()
            if (hasDispatcher === 0) {
                await admin.page
                    .getByTestId("procurement-responsibility-create")
                    .click()
                await expect(
                    admin.page.getByRole("heading", { name: "新增采购责任规则" }),
                ).toBeVisible(VISIBLE)
                await chooseComboboxOption(
                    admin.page,
                    admin.page.locator(
                        "#procurement-responsibility-rules-dialog-rule-type",
                    ),
                    "默认调度人",
                    "默认调度人",
                )
                await chooseComboboxOption(
                    admin.page,
                    admin.page.locator(
                        "#procurement-responsibility-rules-dialog-owner",
                    ),
                    "caigou",
                    /采购.*caigou|caigou/,
                )
                await admin.page
                    .getByTestId("procurement-responsibility-save")
                    .click()
                await expectToast(admin.page, "采购责任规则已新增")
            }
        } finally {
            await admin.context.close()
        }
    }

    // 2. 销售：客户 + 合同 PDF + 实物销售单提交
    const sales = await openSession(browser, "xiaoshou")
    try {
        await sales.page.goto("/sales/customers")
        await expect(
            sales.page.getByRole("heading", { name: "客户中心" }),
        ).toBeVisible(VISIBLE)
        await sales.page.locator("#customers-directory-create").click()
        await expect(
            sales.page.getByRole("heading", { name: "新建客户" }),
        ).toBeVisible(VISIBLE)
        await sales.page.getByLabel("法定名称").fill(legalName)
        await sales.page.getByLabel("客户简称").fill(`测试客户${stamp.slice(-6)}`)
        await sales.page.getByLabel("统一社会信用代码").fill(creditCode)
        await sales.page.locator("#customers-form-submit").click()
        await expectToast(sales.page, "客户已创建")
        await expect(
            sales.page.getByRole("heading", { name: "新建客户" }),
        ).toHaveCount(0, VISIBLE)

        await sales.page.goto("/sales/contracts")
        await expect(sales.page.getByRole("heading", { name: "合同" })).toBeVisible(
            VISIBLE,
        )
        await sales.page.getByRole("button", { name: "上传合同 PDF" }).click()
        await expect(
            sales.page.getByRole("heading", { name: "上传合同 PDF" }),
        ).toBeVisible(VISIBLE)
        await sales.page
            .locator("#card-contracts-upload-pdf-input")
            .setInputFiles(SAMPLE_CONTRACT_PDF)
        await sales.page.getByLabel("合同编号").fill(contractNo)
        await chooseComboboxOption(
            sales.page,
            sales.page.locator("#card-contracts-upload-customer"),
            legalName,
            new RegExp(legalName),
        )
        await expect(
            sales.page.locator("#card-contracts-upload-settlement-party"),
        ).not.toHaveValue("", { timeout: 20_000 })
        await sales.page.locator("#card-contracts-upload-submit").click()
        await expectToast(sales.page, "合同 PDF 已归档")

        await sales.page.goto("/sales/orders?mode=create")
        await expect(sales.page.getByText("单据头")).toBeVisible(VISIBLE)
        await chooseComboboxOption(
            sales.page,
            sales.page.locator("#sales-orders-create-contract"),
            contractNo,
            new RegExp(contractNo),
        )
        await expect(sales.page.getByText(legalName)).toBeVisible(VISIBLE)
        await chooseComboboxOption(
            sales.page,
            sales.page.locator("#sales-orders-create-header-welfare-scene"),
            "年节礼包",
            "年节礼包",
        )
        await sales.page.getByRole("button", { name: "选择商品" }).click()
        const skuDialog = sales.page.getByRole("dialog", { name: "选择商品" })
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
        await expect(sales.page.getByText(SKU_NAME)).toBeVisible(VISIBLE)
        await sales.page
            .locator('input[id^="sales-orders-create-line-"][id$="-quantity"]')
            .fill("2")
        await pickDateById(sales.page, "sales-orders-create-batch-due-date")
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
            sales.page.locator("[data-slot=document-header]").getByText("审批中"),
        ).toBeVisible(VISIBLE)
    } finally {
        await sales.context.close()
    }

    // 3. 采购确认销售单（采购确认节点不选供给）
    {
        const procurement = await openSession(browser, "caigou")
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
        const procurement = await openSession(browser, "caigou")
        try {
            await procurement.page.goto("/workspace")
            await expect(
                procurement.page.getByRole("heading", { name: "我的工作台" }),
            ).toBeVisible(VISIBLE)
            await procurement.page.locator("#workspace-family-nav-procurement").click()
            const task = procurement.page
                .getByRole("list", { name: "待办列表" })
                .getByRole("button", { name: /待供给分配/ })
            await expect(task).toBeVisible(VISIBLE)
            await task.click()
            await expect(
                procurement.page.getByRole("region", { name: "当前供给分配任务" }),
            ).toBeVisible(VISIBLE)
            await expect(
                procurement.page.getByText("销售明细与供给方案"),
            ).toBeVisible(VISIBLE)
            await procurement.page
                .getByTestId("purchase-create-match-best")
                .click()
            await expect(
                procurement.page.getByText("销售明细与供给方案"),
            ).toBeVisible(VISIBLE)
            const warehouseInput = procurement.page.getByPlaceholder("选择目标仓")
            if ((await warehouseInput.count()) > 0) {
                await chooseComboboxOption(
                    procurement.page,
                    warehouseInput,
                    WAREHOUSE_NAME,
                    new RegExp(WAREHOUSE_NAME),
                )
            }
            await fillEmptyDatePickers(procurement.page)
            await procurement.page.getByTestId("purchase-create-preview").click()
            await expect(
                procurement.page.getByRole("heading", { name: "预览供给分配" }),
            ).toBeVisible(VISIBLE)
            await expect(
                procurement.page.getByText("本次不占用现有库存"),
            ).toBeVisible(VISIBLE)
            await expect(
                procurement.page.getByText("无需创建采购单"),
            ).toHaveCount(0)
            await procurement.page
                .locator("#procurement-orders-create-preview-confirm")
                .click()
            await expect(
                procurement.page.getByRole("heading", { name: "确认供给分配" }),
            ).toBeVisible(VISIBLE)
            await procurement.page
                .getByTestId("purchase-create-confirm")
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
        const finance = await openSession(browser, "caiwu")
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

    const procurement = await openSession(browser, "caigou")
    let purchaseHref = ""
    try {
        await procurement.page.goto("/procurement/orders")
        await expect(
            procurement.page.getByRole("heading", { name: "采购单" }),
        ).toBeVisible(VISIBLE)
        const openPo = procurement.page.getByRole("link", {
            name: /打开采购单/,
        })
        await expect(openPo).toBeVisible(VISIBLE)
        await openPo.click()
        await expect(procurement.page).toHaveURL(
            /\/procurement\/orders\/[^/?#]+/,
            VISIBLE,
        )
        purchaseHref = procurement.page.url()
        await expect(procurement.page.getByText("已生效")).toBeVisible(VISIBLE)
        await expect(procurement.page.getByText("v1")).toBeVisible(VISIBLE)
        await expect(procurement.page.getByText("未付")).toBeVisible(VISIBLE)
        await expect(procurement.page.getByText("未开始")).toBeVisible(VISIBLE)
        await expect(
            procurement.page.getByRole("button", { name: "去交付" }),
        ).toBeVisible(VISIBLE)
        await expect(
            procurement.page.getByRole("button", { name: "去供应商往来" }),
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
        await expect(procurement.page.getByText("草稿")).toBeVisible(VISIBLE)
        await expect(
            procurement.page.getByText("采购变更").or(procurement.page.getByText("基准 v1")),
        ).toBeVisible(VISIBLE)

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
        await expect(procurement.page.getByText("审批中")).toBeVisible(VISIBLE)
        // 提交后原采购版本仍有效
        await expect(procurement.page.getByText("已生效")).toBeVisible(VISIBLE)
        await expect(procurement.page.getByText("v1")).toBeVisible(VISIBLE)
    } finally {
        await procurement.context.close()
    }

    // 8. 仓储确认库存发货影响（统一审批第一节点，W01 原地处理）
    {
        const warehouse = await openSession(browser, "cangchu")
        try {
            await warehouse.page.goto("/workspace")
            await expect(
                warehouse.page.getByRole("heading", { name: "我的工作台" }),
            ).toBeVisible(VISIBLE)
            const fulfillmentTask = warehouse.page.getByRole("button", {
                name: /履约处理/,
            })
            if ((await fulfillmentTask.count()) > 0) {
                await warehouse.page
                    .locator("#workspace-family-nav-approval")
                    .click()
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
        const finance = await openSession(browser, "caiwu")
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
        const procurement = await openSession(browser, "caigou")
        try {
            await procurement.page.goto(purchaseHref)
            await expect(procurement.page.getByText("已生效")).toBeVisible(VISIBLE)
            await expect(procurement.page.getByText("v2")).toBeVisible(VISIBLE)
            await procurement.page
                .getByRole("tab", { name: "变更与异常" })
                .click()
            await expect(procurement.page.getByText("已生效")).toBeVisible(VISIBLE)
            await expect(
                procurement.page.getByRole("button", { name: "提交改单" }),
            ).toHaveCount(0)
            await procurement.page.getByRole("tab", { name: "应付与票款" }).click()
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
        const warehouse = await openSession(browser, "cangchu")
        try {
            await warehouse.page.goto("/workspace")
            await expect(
                warehouse.page.getByRole("heading", { name: "我的工作台" }),
            ).toBeVisible(VISIBLE)
            await warehouse.page.locator("#workspace-family-nav-approval").click()
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
