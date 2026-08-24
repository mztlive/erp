import path from "node:path";

import {
  expect,
  test,
  type APIRequestContext,
  type Locator,
  type Page,
} from "@playwright/test";

import { publishTwoStepSalesOrderApproval } from "../helpers/approval";
import { api, apiLogin, apiProfile } from "../helpers/api";
import { createSinglePageAccountSwitcher } from "../helpers/login";
import { gotoPage } from "../helpers/ui";
import {
  approveWorkspaceTaskByDocumentNo,
  openVisibleWorkspaceDocument,
  openWorkspaceTask,
  workspaceTaskDetail,
} from "../helpers/workspace";

const CONTRACT_PDF = path.join(
  __dirname,
  "..",
  "fixtures",
  "sample-contract.pdf",
);
const PROCUREMENT_TASK_TYPE = "PROCUREMENT_ORDER_CREATION";

type SellableSku = {
  sku_id: string;
  sku_no: string;
  name: string;
  product_kind: string;
  supplier_count: number;
};

type SupplierOffering = {
  status?: string | null;
  availability_status?: string | null;
  available_quantity?: string | null;
  dropship_supply_price_gross?: string | null;
};

type ProcurementRule = {
  id: string;
  rule_type: string;
  sku_id?: string | null;
  owner_user_id: string;
  status?: string;
  version: number;
};

type WorkItem = {
  id: string;
  work_item_type: string;
  status: "OPEN" | "COMPLETED" | "CLOSED";
  business_object_id: string;
  owner_user_id?: string | null;
  assignment_source?: string;
};

type SalesOrderDetail = {
  id: string;
  order_no: string;
  commercial_status: string;
  purchase_order_count: number;
  purchase_coverage: {
    total_quantity: string;
    covered_quantity: string;
    remaining_quantity: string;
  };
};

type ApprovalInstance = {
  document_id?: string | null;
  status: string;
  current_node_name?: string | null;
};

type CreationBasisLine = {
  sales_order_line_id: string;
  sales_order_submission_line_id?: string;
  sales_quantity: string;
  covered_quantity: string;
  remaining_quantity: string;
  max_create_quantity?: string;
  max_purchasable_quantity?: string;
};

type CreationBasis = {
  basis_id: string;
  work_item_id: string;
  sales_order_id: string;
  sales_order_no: string;
  supplier_id: string;
  purchase_type: string;
  payment_term_code: string;
  lines: CreationBasisLine[];
};

type PurchaseOrderListItem = {
  id: string;
  sales_order_id: string;
  status: string;
  version: number;
};

type PurchaseOrderDetail = {
  id: string;
  sales_order_id: string;
  status: string;
  version: number;
  lines: Array<{
    line_type?: string;
    quantity?: string | null;
    allocated_quantity?: string | null;
  }>;
};

function suffix(): string {
  return Date.now().toString().slice(-10);
}

function numericQuantity(value: string | number | null | undefined): number {
  const quantity = Number(value);
  if (!Number.isFinite(quantity)) {
    throw new Error(`非法数量: ${String(value)}`);
  }
  return quantity;
}

async function pickComboboxOption(
  page: Page,
  input: Locator,
  optionText: string,
): Promise<void> {
  await input.click();
  await input.fill(optionText);
  const option = page
    .locator('[data-slot="combobox-item"]')
    .filter({ hasText: optionText })
    .first();
  await expect(option).toBeVisible({ timeout: 20_000 });
  await option.click();
}

function fieldBox(page: Page, label: string): Locator {
  return page
    .locator('[data-slot="field"]')
    .filter({ has: page.getByText(label, { exact: true }) })
    .first();
}

async function pickToday(page: Page, label: string): Promise<void> {
  const field = fieldBox(page, label);
  const trigger = field
    .getByRole("button", { name: /^(选择日期|已选日期)/ })
    .first();
  await expect(trigger).toBeVisible({ timeout: 20_000 });
  await trigger.click();
  const calendar = page.locator('[data-slot="calendar"]').last();
  await expect(calendar).toBeVisible({ timeout: 10_000 });
  const dayAttribute = await page.evaluate(() =>
    new Date().toLocaleDateString(),
  );
  const today = calendar.locator(`[data-day=${JSON.stringify(dayAttribute)}]`);
  await expect(today).toBeVisible({ timeout: 10_000 });
  const selected =
    (await today.getAttribute("data-selected-single")) === "true" ||
    (await today.getAttribute("aria-selected")) === "true";
  if (selected) {
    await page.keyboard.press("Escape");
  } else {
    await today.click();
  }
  await expect(calendar).not.toBeVisible({ timeout: 10_000 });
}

async function findProcureableSku(
  request: APIRequestContext,
  adminToken: string,
): Promise<SellableSku> {
  const skuPage = await api<{ items: SellableSku[] }>(
    request,
    "GET",
    "/admin/sellable-skus",
    {
      token: adminToken,
      query: { product_kind: "PHYSICAL", page: 1, page_size: 100 },
    },
  );
  for (const sku of skuPage.items ?? []) {
    if (Number(sku.supplier_count) < 1) continue;
    const offeringPage = await api<{ items: SupplierOffering[] }>(
      request,
      "GET",
      "/admin/supplier-offerings",
      {
        token: adminToken,
        query: {
          sku_id: sku.sku_id,
          availability_status: "AVAILABLE",
          page: 1,
          page_size: 100,
        },
      },
    );
    const qualified = (offeringPage.items ?? []).some((offering) => {
      const available = offering.available_quantity;
      return (
        (!offering.status || offering.status === "ACTIVE") &&
        offering.availability_status === "AVAILABLE" &&
        Boolean(offering.dropship_supply_price_gross) &&
        (available == null || numericQuantity(available) >= 10)
      );
    });
    if (qualified) return sku;
  }
  throw new Error(
    "未找到可供数量至少 10 且支持供应商直发的 PHYSICAL SKU；请检查 E2E 保留主数据",
  );
}

async function upsertSkuResponsibilityRule(
  request: APIRequestContext,
  adminToken: string,
  skuId: string,
  procurementUserId: string,
): Promise<ProcurementRule> {
  const page = await api<{ items: ProcurementRule[] }>(
    request,
    "GET",
    "/admin/procurement-responsibility-rules",
    {
      token: adminToken,
      query: { rule_type: "SKU", page: 1, page_size: 200 },
    },
  );
  const existing = (page.items ?? []).find((rule) => rule.sku_id === skuId);
  const body = {
    rule_type: "SKU",
    sku_id: skuId,
    category_id: null,
    service_region: null,
    product_kind: null,
    owner_user_id: procurementUserId,
    status: "active",
  };
  const saved = existing
    ? await api<ProcurementRule>(
        request,
        "PUT",
        `/admin/procurement-responsibility-rules/${encodeURIComponent(existing.id)}`,
        {
          token: adminToken,
          body: { ...body, version: existing.version },
        },
      )
    : await api<ProcurementRule>(
        request,
        "POST",
        "/admin/procurement-responsibility-rules",
        { token: adminToken, body },
      );

  const preview = await api<{
    lines: Array<{
      line_key: string;
      resolved: boolean;
      owner_user_id?: string | null;
      rule_id?: string | null;
    }>;
  }>(request, "POST", "/admin/procurement-responsibility/resolve", {
    token: adminToken,
    body: {
      lines: [
        {
          line_key: "e2e-procurement-owner-preview",
          sku_id: skuId,
          service_region: null,
        },
      ],
    },
  });
  expect(preview.lines).toEqual([
    expect.objectContaining({
      resolved: true,
      owner_user_id: procurementUserId,
      rule_id: saved.id,
    }),
  ]);
  return saved;
}

async function listProcurementTasks(
  request: APIRequestContext,
  token: string,
  scope: "mine" | "history",
  status?: "COMPLETED" | "CLOSED",
): Promise<WorkItem[]> {
  const query: Record<string, unknown> = {
    scope,
    work_item_type: PROCUREMENT_TASK_TYPE,
    timezone: "Asia/Shanghai",
    page: 1,
    page_size: 100,
  };
  if (status) query.status = status;
  const page = await api<{ items: WorkItem[] }>(
    request,
    "GET",
    "/admin/work-items",
    { token, query },
  );
  return page.items ?? [];
}

async function findApprovalInstance(
  request: APIRequestContext,
  salesToken: string,
  salesOrderId: string,
): Promise<ApprovalInstance> {
  const page = await api<{ items: ApprovalInstance[] }>(
    request,
    "GET",
    "/admin/approval-instances",
    {
      token: salesToken,
      query: { view: "started", document_type: "sales_order", limit: 50 },
    },
  );
  const instance = (page.items ?? []).find(
    (item) => item.document_id === salesOrderId,
  );
  expect(instance, `应找到销售单 ${salesOrderId} 的审批实例`).toBeTruthy();
  return instance!;
}

async function salesOrderDetail(
  request: APIRequestContext,
  token: string,
  salesOrderId: string,
): Promise<SalesOrderDetail> {
  return api<SalesOrderDetail>(
    request,
    "GET",
    `/admin/sales-orders/${encodeURIComponent(salesOrderId)}`,
    { token },
  );
}

async function creationBases(
  request: APIRequestContext,
  token: string,
  salesOrderId: string,
  workItemId: string,
): Promise<CreationBasis[]> {
  const rows = await api<CreationBasis[]>(
    request,
    "GET",
    "/admin/purchase-creation-bases",
    {
      token,
      query: {
        sales_order_id: salesOrderId,
        work_item_id: workItemId,
      },
    },
  );
  return (rows ?? []).filter((basis) => basis.sales_order_id === salesOrderId);
}

function createFromBasisBody(
  basis: CreationBasis,
  workItemId: string,
  quantity: string,
  idempotencyKey: string,
): Record<string, unknown> {
  const line = basis.lines[0];
  if (!line) throw new Error("采购创建依据缺少明细");
  return {
    work_item_id: workItemId,
    basis_id: basis.basis_id,
    purchase_type: basis.purchase_type,
    payment_term_code: basis.payment_term_code,
    lines: [
      {
        sales_order_line_id: line.sales_order_line_id,
        quantity,
      },
    ],
    idempotency_key: idempotencyKey,
  };
}

async function expectCoverage(
  request: APIRequestContext,
  token: string,
  salesOrderId: string,
  expected: {
    purchaseOrders: number;
    total: number;
    covered: number;
    remaining: number;
  },
): Promise<void> {
  await expect
    .poll(
      async () => {
        const detail = await salesOrderDetail(request, token, salesOrderId);
        return {
          purchaseOrders: Number(detail.purchase_order_count),
          total: numericQuantity(detail.purchase_coverage.total_quantity),
          covered: numericQuantity(detail.purchase_coverage.covered_quantity),
          remaining: numericQuantity(
            detail.purchase_coverage.remaining_quantity,
          ),
        };
      },
      { timeout: 30_000 },
    )
    .toEqual(expected);
}

async function purchaseOrderQuantities(
  request: APIRequestContext,
  token: string,
  salesOrderId: string,
): Promise<number[]> {
  const page = await api<{ items: PurchaseOrderListItem[] }>(
    request,
    "GET",
    "/admin/purchase-orders",
    {
      token,
      query: { sales_order_id: salesOrderId, page: 1, page_size: 100 },
    },
  );
  const orders = (page.items ?? []).filter(
    (order) =>
      order.sales_order_id === salesOrderId && order.status !== "VOIDED",
  );
  const details = await Promise.all(
    orders.map((order) =>
      api<PurchaseOrderDetail>(
        request,
        "GET",
        `/admin/purchase-orders/${encodeURIComponent(order.id)}`,
        { token },
      ),
    ),
  );
  return details
    .map((order) =>
      order.lines
        .filter((line) => line.line_type !== "LOGISTICS_FEE")
        .reduce(
          (sum, line) =>
            sum +
            numericQuantity(line.allocated_quantity ?? line.quantity ?? "0"),
          0,
        ),
    )
    .sort((left, right) => left - right);
}

async function purchaseOrderByQuantity(
  request: APIRequestContext,
  token: string,
  salesOrderId: string,
  quantity: number,
): Promise<PurchaseOrderDetail> {
  const page = await api<{ items: PurchaseOrderListItem[] }>(
    request,
    "GET",
    "/admin/purchase-orders",
    {
      token,
      query: { sales_order_id: salesOrderId, page: 1, page_size: 100 },
    },
  );
  const details = await Promise.all(
    (page.items ?? [])
      .filter(
        (order) =>
          order.sales_order_id === salesOrderId && order.status !== "VOIDED",
      )
      .map((order) =>
        api<PurchaseOrderDetail>(
          request,
          "GET",
          `/admin/purchase-orders/${encodeURIComponent(order.id)}`,
          { token },
        ),
      ),
  );
  const matched = details.find((order) => {
    const allocated = order.lines
      .filter((line) => line.line_type !== "LOGISTICS_FEE")
      .reduce(
        (sum, line) =>
          sum +
          numericQuantity(line.allocated_quantity ?? line.quantity ?? "0"),
        0,
      );
    return allocated === quantity;
  });
  if (!matched) {
    throw new Error(`未找到采购数量为 ${quantity} 的采购单`);
  }
  return matched;
}

function basisLocator(dialog: Locator, basis: CreationBasis): Locator {
  return dialog.getByTestId(`purchase-basis-${basis.basis_id}`);
}

function basisQuantity(
  dialog: Locator,
  basis: CreationBasis,
  line: CreationBasisLine = basis.lines[0]!,
): Locator {
  return basisLocator(dialog, basis).getByTestId(
    `purchase-basis-line-quantity-${line.sales_order_line_id}`,
  );
}

test("采购责任在销售最终生效后派发建单任务，并按 4 + 6 建立两张采购草稿", async ({
  page,
  request,
}) => {
  test.setTimeout(600_000);

  const switchAccount = createSinglePageAccountSwitcher(page);
  const stamp = suffix();
  const customerLegalName = `E2E采购责任客户${stamp}`;
  const customerShortName = `采购责${stamp.slice(-6)}`;
  const creditCode = `91${stamp.padEnd(16, "0").slice(0, 16)}`;
  const contractNo = `HT-PRMPO-${stamp}`;

  const [adminToken, salesToken, procurementToken] = await Promise.all([
    apiLogin(request, "admin"),
    apiLogin(request, "sales"),
    apiLogin(request, "procurement"),
  ]);
  const procurementProfile = await apiProfile(request, procurementToken);
  const sku = await findProcureableSku(request, adminToken);

  await test.step("发布采购确认非末级的两级销售审批并配置 SKU 采购负责人", async () => {
    await publishTwoStepSalesOrderApproval(request);
    const rule = await upsertSkuResponsibilityRule(
      request,
      adminToken,
      sku.sku_id,
      procurementProfile.userid,
    );
    expect(rule.owner_user_id).toBe(procurementProfile.userid);
  });

  await test.step("管理员通过责任规则页面核对 SKU 与具体采购负责人并保存", async () => {
    await switchAccount("admin");
    await gotoPage(page, "/master-data/procurement-responsibilities");
    const row = page.getByRole("row").filter({ hasText: sku.sku_no }).first();
    await expect(row).toContainText(procurementProfile.name, {
      timeout: 30_000,
    });
    await row.getByRole("button", { name: "编辑" }).click();
    const dialog = page.getByRole("dialog").last();
    await expect(
      dialog.getByRole("heading", { name: "编辑采购责任规则" }),
    ).toBeVisible({ timeout: 20_000 });
    await pickComboboxOption(page, dialog.getByLabel("公司 SKU"), sku.sku_no);
    await pickComboboxOption(
      page,
      dialog.getByRole("combobox", { name: "采购负责人" }),
      procurementProfile.name,
    );
    await dialog.getByTestId("procurement-responsibility-save").click();
    await expect(dialog).not.toBeVisible({ timeout: 30_000 });
    await expect(row).toContainText(procurementProfile.name);
  });

  let salesOrderId = "";
  let salesOrderNo = "";

  await test.step("销售创建数量 10 的外采销售单并看到只读采购负责人", async () => {
    await switchAccount("sales");
    await gotoPage(page, "/sales/customers");
    await page.getByRole("button", { name: "新建客户" }).first().click();
    const customerDialog = page.getByRole("dialog").last();
    await expect(customerDialog).toBeVisible({ timeout: 20_000 });
    await customerDialog.getByLabel("法定名称").fill(customerLegalName);
    await customerDialog.getByLabel("客户简称").fill(customerShortName);
    await customerDialog.getByLabel("统一社会信用代码").fill(creditCode);
    await pickComboboxOption(
      page,
      customerDialog.getByLabel("默认付款条件"),
      "货到 15 天",
    );
    await customerDialog.getByRole("button", { name: "创建客户" }).click();
    await expect(customerDialog).not.toBeVisible({ timeout: 20_000 });
    await page
      .getByRole("link", { name: customerShortName, exact: true })
      .click();
    await expect(page).toHaveURL(/\/sales\/customers\/[0-9a-f]{24,32}/, {
      timeout: 20_000,
    });

    await gotoPage(page, "/sales/contracts");
    await page.getByRole("button", { name: "上传合同 PDF" }).click();
    const contractDialog = page.getByRole("dialog").last();
    await expect(
      contractDialog.getByRole("heading", { name: "上传合同 PDF" }),
    ).toBeVisible({ timeout: 20_000 });
    await contractDialog
      .locator('input[type="file"]')
      .setInputFiles(CONTRACT_PDF);
    await contractDialog.getByLabel("合同编号").fill(contractNo);
    await pickComboboxOption(
      page,
      contractDialog.getByPlaceholder("搜索客户编号或名称"),
      customerLegalName,
    );
    await expect(
      contractDialog.getByRole("combobox", { name: "结算主体" }),
    ).toHaveValue(customerLegalName, { timeout: 20_000 });
    await contractDialog.getByRole("button", { name: "上传并归档" }).click();
    await expect(contractDialog).not.toBeVisible({ timeout: 20_000 });

    await gotoPage(page, "/sales/orders?mode=create");
    await pickComboboxOption(
      page,
      page.getByPlaceholder("搜索合同编号或客户").first(),
      contractNo,
    );
    await pickComboboxOption(page, page.getByLabel("福利场景"), "年节礼包");
    await pickComboboxOption(page, page.getByLabel("履约方式"), "供应商直发");
    await pickComboboxOption(
      page,
      page.getByPlaceholder("搜索 SKU 或商品名称").first(),
      sku.sku_no,
    );
    await page.getByLabel("数量").fill("10");
    await page.getByLabel("含税单价").fill("100");
    await pickToday(page, "交付日期");

    const owner = page
      .locator('[data-testid^="sales-line-procurement-owner-"]')
      .first();
    await expect(owner).toBeVisible({ timeout: 20_000 });
    await expect(owner).toContainText(procurementProfile.name);
    await expect(owner.getByRole("combobox")).toHaveCount(0);

    await page.getByTestId("sales-order-submit").click();
    const submitDialog = page.getByRole("alertdialog").last();
    await expect(submitDialog).toBeVisible({ timeout: 20_000 });
    await submitDialog.getByRole("button", { name: "确认提交" }).click();
    await expect(page).toHaveURL(/\/sales\/orders\/[0-9a-f]{24,32}/, {
      timeout: 30_000,
    });
    salesOrderId = new URL(page.url()).pathname.split("/").pop() ?? "";
    const detail = await salesOrderDetail(request, salesToken, salesOrderId);
    salesOrderNo = detail.order_no;
    expect(salesOrderNo).toMatch(/^XS/);
    await expect(page.getByText("审批中", { exact: true }).first()).toBeVisible(
      { timeout: 20_000 },
    );
  });

  await test.step("采购确认通过后仍停在销售领导审批，且没有采购建单任务", async () => {
    await switchAccount("procurement");
    await gotoPage(page, "/workspace");
    await approveWorkspaceTaskByDocumentNo(page, salesOrderNo);

    const instance = await findApprovalInstance(
      request,
      salesToken,
      salesOrderId,
    );
    expect(instance.status).not.toBe("APPROVED");
    expect(instance.current_node_name).toBe("销售领导审批");
    expect(
      (await salesOrderDetail(request, salesToken, salesOrderId))
        .commercial_status,
    ).not.toBe("EFFECTIVE");
    expect(
      (await listProcurementTasks(request, procurementToken, "mine")).filter(
        (item) => item.business_object_id === salesOrderId,
      ),
    ).toHaveLength(0);
    await expect(
      page
        .locator('[data-testid^="work-item-procurement-order-creation-"]')
        .filter({ hasText: salesOrderNo }),
    ).toHaveCount(0);
  });

  let procurementTask: WorkItem;

  await test.step("最后审批通过后销售生效，caigou 收到具体到人的采购建单任务", async () => {
    await switchAccount("salesLeader");
    await gotoPage(page, "/workspace");
    await approveWorkspaceTaskByDocumentNo(page, salesOrderNo);

    await expect
      .poll(
        async () =>
          (await salesOrderDetail(request, salesToken, salesOrderId))
            .commercial_status,
        { timeout: 30_000 },
      )
      .toBe("EFFECTIVE");
    await expect
      .poll(
        async () => {
          const tasks = await listProcurementTasks(
            request,
            procurementToken,
            "mine",
          );
          return tasks.find((item) => item.business_object_id === salesOrderId)
            ?.id;
        },
        { timeout: 30_000 },
      )
      .toBeTruthy();
    procurementTask = (
      await listProcurementTasks(request, procurementToken, "mine")
    ).find((item) => item.business_object_id === salesOrderId)!;
    expect(procurementTask).toEqual(
      expect.objectContaining({
        work_item_type: PROCUREMENT_TASK_TYPE,
        status: "OPEN",
        owner_user_id: procurementProfile.userid,
        assignment_source: "SYSTEM_RULE",
      }),
    );

    await switchAccount("procurement");
    await gotoPage(page, "/workspace");
    const task = page.getByTestId(
      `work-item-procurement-order-creation-${procurementTask.id}`,
    );
    await expect(task).toContainText(salesOrderNo);
    await openWorkspaceTask(page, task);
    await expect(
      page.getByTestId(`work-item-open-document-${procurementTask.id}`),
    ).toBeVisible();
    await openVisibleWorkspaceDocument(page);
    await expect(page).toHaveURL(
      new RegExp(
        `/procurement/orders\\?.*salesOrderId=${salesOrderId}.*workItemId=${procurementTask.id}`,
      ),
      { timeout: 20_000 },
    );
  });

  await test.step("第一次建立数量 4 的采购草稿，销售与任务剩余数量变为 6", async () => {
    const dialog = page.getByRole("dialog").last();
    await expect(
      dialog.getByRole("heading", { name: "从采购创建依据建单" }),
    ).toBeVisible({ timeout: 20_000 });
    const bases = await creationBases(
      request,
      procurementToken,
      salesOrderId,
      procurementTask.id,
    );
    expect(bases.length).toBeGreaterThan(0);
    const basis = bases[0]!;
    expect(basis.work_item_id).toBe(procurementTask.id);
    expect(basis.sales_order_no).toBe(salesOrderNo);
    expect(numericQuantity(basis.lines[0]?.sales_quantity)).toBe(10);
    expect(numericQuantity(basis.lines[0]?.covered_quantity)).toBe(0);
    expect(numericQuantity(basis.lines[0]?.remaining_quantity)).toBe(10);

    await expect(
      creationBases(request, adminToken, salesOrderId, procurementTask.id),
    ).resolves.toHaveLength(0);
    await expect(
      api(request, "POST", "/admin/purchase-orders", {
        token: procurementToken,
        body: createFromBasisBody(
          basis,
          `forged-${procurementTask.id}`,
          "1",
          `e2e-forged-task-${stamp}`,
        ),
      }),
    ).rejects.toThrow(/HTTP 404/);
    await expect(
      api(request, "POST", "/admin/purchase-orders", {
        token: adminToken,
        body: createFromBasisBody(
          basis,
          procurementTask.id,
          "1",
          `e2e-cross-owner-${stamp}`,
        ),
      }),
    ).rejects.toThrow(/HTTP (403|404)/);
    await expectCoverage(request, salesToken, salesOrderId, {
      purchaseOrders: 0,
      total: 10,
      covered: 0,
      remaining: 10,
    });

    const quantity = basisQuantity(dialog, basis);
    await expect(quantity).toHaveValue("10", { timeout: 20_000 });
    await quantity.fill("4");
    await dialog.getByTestId("purchase-create-from-basis").click();
    await expect(dialog.getByTestId("purchase-create-result")).toContainText(
      "已创建采购草稿",
      { timeout: 30_000 },
    );
    const remainingBases = await creationBases(
      request,
      procurementToken,
      salesOrderId,
      procurementTask.id,
    );
    const remainingBasis = remainingBases[0]!;
    expect(remainingBasis.basis_id).not.toBe(basis.basis_id);
    await expect(basisQuantity(dialog, remainingBasis)).toHaveValue("6", {
      timeout: 30_000,
    });

    await expectCoverage(request, salesToken, salesOrderId, {
      purchaseOrders: 1,
      total: 10,
      covered: 4,
      remaining: 6,
    });
    await expect
      .poll(
        async () => {
          const remainingBases = await creationBases(
            request,
            procurementToken,
            salesOrderId,
            procurementTask.id,
          );
          const line = remainingBases[0]?.lines[0];
          return line
            ? {
                covered: numericQuantity(line.covered_quantity),
                remaining: numericQuantity(line.remaining_quantity),
              }
            : null;
        },
        { timeout: 30_000 },
      )
      .toEqual({ covered: 4, remaining: 6 });

    await page.setViewportSize({ width: 390, height: 844 });
    await gotoPage(page, `/sales/orders/${salesOrderId}`);
    const mobileProgress = page.getByTestId("sales-order-procurement-progress");
    await expect(mobileProgress).toContainText(
      "销售总数量 10 · 已覆盖 4 · 剩余 6",
      { timeout: 20_000 },
    );
    await expect(mobileProgress).toContainText("1 笔");
    const mobileContinue = page.getByTestId("sales-order-continue-purchase");
    await expect(mobileContinue).toBeVisible();
    await mobileContinue.click();
    await expect(page).toHaveURL(
      new RegExp(
        `/procurement/orders\\?.*salesOrderId=${salesOrderId}.*action=create`,
      ),
      { timeout: 20_000 },
    );
    await expect(
      page
        .getByRole("dialog")
        .last()
        .getByRole("heading", { name: "从采购创建依据建单" }),
    ).toBeVisible({ timeout: 20_000 });
    await gotoPage(page, "/workspace");
    const task = page.getByTestId(
      `work-item-procurement-order-creation-${procurementTask.id}`,
    );
    await openWorkspaceTask(page, task);
    await expect(workspaceTaskDetail(page)).toContainText(/剩余[^0-9]*6/);
    await openVisibleWorkspaceDocument(page);
  });

  await test.step("第二次建立数量 6 的另一张采购草稿，剩余归零且任务完成", async () => {
    const dialog = page.getByRole("dialog").last();
    await expect(
      dialog.getByRole("heading", { name: "从采购创建依据建单" }),
    ).toBeVisible({ timeout: 20_000 });
    const bases = await creationBases(
      request,
      procurementToken,
      salesOrderId,
      procurementTask.id,
    );
    const basis = bases[0]!;
    const quantity = basisQuantity(dialog, basis);
    await expect(quantity).toHaveValue("6", { timeout: 20_000 });
    await quantity.fill("6");
    await dialog.getByTestId("purchase-create-from-basis").click();
    await expect(dialog).not.toBeVisible({ timeout: 30_000 });
    await expect(page.getByText("当前销售单的可采购数量已覆盖")).toBeVisible({
      timeout: 20_000,
    });
    await page.setViewportSize({ width: 1440, height: 1000 });

    await expectCoverage(request, salesToken, salesOrderId, {
      purchaseOrders: 2,
      total: 10,
      covered: 10,
      remaining: 0,
    });
    await expect
      .poll(
        () => purchaseOrderQuantities(request, procurementToken, salesOrderId),
        { timeout: 30_000 },
      )
      .toEqual([4, 6]);
    expect(
      await creationBases(
        request,
        procurementToken,
        salesOrderId,
        procurementTask.id,
      ),
    ).toHaveLength(0);

    await expect
      .poll(
        async () =>
          (
            await api<WorkItem>(
              request,
              "GET",
              `/admin/work-items/${encodeURIComponent(procurementTask.id)}`,
              { token: procurementToken },
            )
          ).status,
        { timeout: 30_000 },
      )
      .toBe("COMPLETED");
    const history = await listProcurementTasks(
      request,
      procurementToken,
      "history",
      "COMPLETED",
    );
    expect(history.map((item) => item.id)).toContain(procurementTask.id);

    await gotoPage(page, `/sales/orders/${salesOrderId}`);
    const progress = page.getByTestId("sales-order-procurement-progress");
    await expect(progress).toContainText("销售总数量 10 · 已覆盖 10 · 剩余 0", {
      timeout: 20_000,
    });
    await expect(progress).toContainText("2 笔");
    await expect(page.getByTestId("sales-order-continue-purchase")).toHaveCount(
      0,
    );

    await gotoPage(page, "/workspace");
    await expect(
      page.getByTestId(
        `work-item-procurement-order-creation-${procurementTask.id}`,
      ),
    ).toHaveCount(0);
  });

  await test.step("作废数量 4 的采购草稿后释放覆盖并创建新的采购任务，再补建数量 4", async () => {
    const orderToVoid = await purchaseOrderByQuantity(
      request,
      procurementToken,
      salesOrderId,
      4,
    );
    await gotoPage(page, `/procurement/orders/${orderToVoid.id}`);
    await page.getByRole("button", { name: "作废草稿" }).click();
    const voidDialog = page.getByRole("alertdialog").last();
    await expect(
      voidDialog.getByRole("heading", { name: "作废采购草稿" }),
    ).toBeVisible({ timeout: 20_000 });
    await voidDialog.getByRole("button", { name: "确认作废" }).click();
    await expect(page.getByText("已作废", { exact: true }).first()).toBeVisible(
      {
        timeout: 30_000,
      },
    );

    await expectCoverage(request, salesToken, salesOrderId, {
      purchaseOrders: 1,
      total: 10,
      covered: 6,
      remaining: 4,
    });
    expect(
      (
        await api<WorkItem>(
          request,
          "GET",
          `/admin/work-items/${encodeURIComponent(procurementTask.id)}`,
          { token: procurementToken },
        )
      ).status,
    ).toBe("COMPLETED");

    let successorTask: WorkItem | undefined;
    await expect
      .poll(
        async () => {
          successorTask = (
            await listProcurementTasks(request, procurementToken, "mine")
          ).find(
            (item) =>
              item.business_object_id === salesOrderId &&
              item.id !== procurementTask.id,
          );
          return successorTask?.id;
        },
        { timeout: 30_000 },
      )
      .toBeTruthy();
    expect(successorTask).toEqual(
      expect.objectContaining({
        status: "OPEN",
        owner_user_id: procurementProfile.userid,
      }),
    );

    await gotoPage(page, "/workspace");
    const successorCard = page.getByTestId(
      `work-item-procurement-order-creation-${successorTask!.id}`,
    );
    await expect(successorCard).toContainText(salesOrderNo);
    await openWorkspaceTask(page, successorCard);
    await expect(workspaceTaskDetail(page)).toContainText(/剩余[^0-9]*4/);
    await openVisibleWorkspaceDocument(page);
    const dialog = page.getByRole("dialog").last();
    const successorBases = await creationBases(
      request,
      procurementToken,
      salesOrderId,
      successorTask!.id,
    );
    const successorBasis = successorBases[0]!;
    await expect(basisQuantity(dialog, successorBasis)).toHaveValue("4", {
      timeout: 20_000,
    });
    const concurrentResults = await Promise.allSettled([
      api(request, "POST", "/admin/purchase-orders", {
        token: procurementToken,
        body: createFromBasisBody(
          successorBasis,
          successorTask!.id,
          "4",
          `e2e-concurrent-a-${stamp}`,
        ),
      }),
      api(request, "POST", "/admin/purchase-orders", {
        token: procurementToken,
        body: createFromBasisBody(
          successorBasis,
          successorTask!.id,
          "4",
          `e2e-concurrent-b-${stamp}`,
        ),
      }),
    ]);
    expect(
      concurrentResults.filter((result) => result.status === "fulfilled"),
    ).toHaveLength(1);
    expect(
      concurrentResults.filter((result) => result.status === "rejected"),
    ).toHaveLength(1);
    expect(
      String(
        concurrentResults.find((result) => result.status === "rejected")
          ?.reason,
      ),
    ).toMatch(/HTTP 409/);
    await page.reload();
    await expect(page.getByText("该销售单当前没有可建采购依据")).toBeVisible({
      timeout: 30_000,
    });

    await expectCoverage(request, salesToken, salesOrderId, {
      purchaseOrders: 2,
      total: 10,
      covered: 10,
      remaining: 0,
    });
    await expect
      .poll(
        () => purchaseOrderQuantities(request, procurementToken, salesOrderId),
        { timeout: 30_000 },
      )
      .toEqual([4, 6]);
    await expect
      .poll(
        async () =>
          (
            await api<WorkItem>(
              request,
              "GET",
              `/admin/work-items/${encodeURIComponent(successorTask!.id)}`,
              { token: procurementToken },
            )
          ).status,
        { timeout: 30_000 },
      )
      .toBe("COMPLETED");
  });
});
