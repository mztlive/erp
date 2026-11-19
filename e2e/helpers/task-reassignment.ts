import { workPage } from "./workspace-page"
import { randomUUID } from "node:crypto";

import { toAutomationIdSegment } from "../../erp-client/lib/automation-id";
import { expect, test, type Page } from "./test";
import { API_BASE, apiGet, apiToken } from "./api";
import { chooseOption } from "./ui";

type Task = {
  id: string;
  task_version: string;
  owner_user_id: string;
  business_object_id: string;
  status: string;
  allowed_actions: string[];
};
type TaskActionReceipt = Pick<Task, "id" | "task_version">;
type ServiceQueueItem = {
  work_item_id: string;
  operation_id: string;
  purchase_order_id: string;
};
type PurchaseIdentity = {
  owner_user_id: string;
  status: string;
  current_revision_id: string;
  revision_no: number;
};

async function command(token: string, taskId: string, body: unknown) {
  const response = await fetch(`${API_BASE}/admin/work-items/${taskId}/reassign`, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(15_000),
  });
  return {
    status: response.status,
    parsed: (await response.json()) as { success: boolean; errorMessage?: string; data: TaskActionReceipt },
  };
}

/** 验收真实服务任务转交、原责任人的办理拒绝及不合格接收人拦截。 */
export async function verifyServiceTaskReassignment(
  managerPage: Page,
  salesOrderId: string,
): Promise<{ taskId: string; ownerUserId: string }> {
  return test.step("管理员转交服务任务：采购责任同步、旧责任失效、旧版本冲突、重复请求保持同一结果", async () => {
    const managerToken = await apiToken("admin");
    const oldOwnerToken = await apiToken("caigou");
    const queue = await apiGet<{ items: ServiceQueueItem[] }>(
      oldOwnerToken,
      "/admin/work-items/fulfillment-queue",
      {
        operation_types: "SERVICE",
        sales_order_id: salesOrderId,
        page: 1,
        page_size: 100,
      },
    );
    expect(queue.items).toHaveLength(1);
    const service = queue.items[0]!;
    const before = await apiGet<Task>(managerToken, `/admin/work-items/${service.work_item_id}`);
    const purchaseBefore = await apiGet<PurchaseIdentity>(
      managerToken,
      `/admin/purchase-orders/${service.purchase_order_id}`,
    );
    const candidates = await apiGet<
      Array<{ user_id: string; account: string; display_name: string }>
    >(managerToken, `/admin/work-items/${before.id}/reassign-candidates`);
    const target = candidates.find((row) => row.account === "admin");
    expect(target, "实际完整责任候选应包含具有全部业务资格的管理员").toBeTruthy();

    await managerPage.goto(`/workspace/tasks/${encodeURIComponent(before.id)}`);
    await workPage(managerPage).locator(`#workspace-fulfillment-reassign-trigger-${toAutomationIdSegment(before.id)}`).click();
    const dialog = workPage(managerPage).getByRole("dialog", { name: "转交履约待办" });
    await chooseOption(
      managerPage,
      dialog.locator('input[id^="workspace-fulfillment-reassign-target-"]'),
      `${target!.display_name} · ${target!.account}`,
    );
    await dialog
      .locator('textarea[id^="workspace-fulfillment-reassign-reason-"]')
      .fill("E2E 核对责任交接及原负责人退出");
    const [response] = await Promise.all([
      managerPage.waitForResponse(
        (reply) =>
          reply.request().method() === "POST" &&
          new URL(reply.url()).pathname === `/admin/work-items/${before.id}/reassign`,
      ),
      dialog.getByRole("button", { name: "确认转交", exact: true }).click(),
    ]);
    const parsed = (await response.json()) as {
      success: boolean;
      data: TaskActionReceipt;
      errorMessage?: string;
    };
    expect(response.status(), parsed.errorMessage).toBeLessThan(400);
    expect(parsed.success).toBe(true);
    const after = parsed.data;
    expect(after.id).toBe(before.id);
    expect(await apiGet<Task>(managerToken, `/admin/work-items/${after.id}`)).toMatchObject({
      owner_user_id: target!.user_id, task_version: after.task_version,
    });
    expect(after.task_version).not.toBe(before.task_version);
    const purchaseAfter = await apiGet<PurchaseIdentity>(
      managerToken,
      `/admin/purchase-orders/${service.purchase_order_id}`,
    );
    expect(purchaseAfter.owner_user_id).toBe(target!.user_id);
    expect(purchaseAfter.status).toBe(purchaseBefore.status);
    expect(purchaseAfter.current_revision_id).toBe(purchaseBefore.current_revision_id);
    expect(purchaseAfter.revision_no).toBe(purchaseBefore.revision_no);
    const oldQueue = await apiGet<{ items: ServiceQueueItem[] }>(
      oldOwnerToken,
      "/admin/work-items/fulfillment-queue",
      {
        operation_types: "SERVICE",
        sales_order_id: salesOrderId,
        page: 1,
        page_size: 100,
      },
    );
    expect(oldQueue.items.some((row) => row.work_item_id === before.id)).toBe(false);
    const newQueue = await apiGet<{ items: ServiceQueueItem[] }>(
      managerToken,
      "/admin/work-items/fulfillment-queue",
      {
        operation_types: "SERVICE",
        sales_order_id: salesOrderId,
        page: 1,
        page_size: 100,
      },
    );
    expect(newQueue.items.some((row) => row.work_item_id === before.id)).toBe(true);
    // 合法现场事实及真实图片必须被当前责任检查拒绝，并回滚服务确认。
    const serviceBefore = await apiGet<{ version: number; status: string }>(
      managerToken,
      `/admin/service-fulfillments/${service.operation_id}`,
    );
    const deniedForm = new FormData();
    const startedAt = Math.floor(Date.now() / 1000) - 3600;
    deniedForm.append(
      "command",
      JSON.stringify({
        version: serviceBefore.version,
        result: "SUCCESS",
        completion_note: "E2E 原责任人不得在交接后确认服务",
        service_location: "北京市朝阳区 E2E 验收现场",
        service_started_at: startedAt,
        service_ended_at: startedAt + 1800,
        quantity: "2",
        evidence_attachment_id: "pending-file:service-evidence",
      }),
    );
    deniedForm.append(
      "pending-file:service-evidence",
      new Blob(
        [
          Buffer.from(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
            "base64",
          ),
        ],
        { type: "image/png" },
      ),
      "reassign-denied.png",
    );
    const denied = await fetch(
      `${API_BASE}/admin/service-fulfillments/${service.operation_id}/confirm`,
      {
        method: "POST",
        headers: { Authorization: `Bearer ${oldOwnerToken}` },
        body: deniedForm,
        signal: AbortSignal.timeout(15000),
      },
    );
    const deniedBody = (await denied.json()) as { errorMessage?: string };
    expect(denied.status, deniedBody.errorMessage).toBeGreaterThanOrEqual(400);
    expect(deniedBody.errorMessage).toMatch(/当前责任人|权限|责任/);
    expect(
      await apiGet<{ version: number; status: string }>(
        managerToken,
        `/admin/service-fulfillments/${service.operation_id}`,
      ),
    ).toMatchObject({ version: serviceBefore.version, status: serviceBefore.status });
    const replay = await command(managerToken, before.id, response.request().postDataJSON());
    expect(replay.status, replay.parsed.errorMessage).toBeLessThan(400);
    expect(replay.parsed.data).toEqual(after);
    const stale = await command(managerToken, before.id, {
      expected_task_version: before.task_version,
      target_user_id: before.owner_user_id,
      reason: "E2E 拒绝旧版本覆盖交接",
      idempotency_key: randomUUID(),
    });
    expect(stale.status).toBe(409);
    expect((await apiGet<Task>(managerToken, `/admin/work-items/${before.id}`)).owner_user_id).toBe(
      target!.user_id,
    );
    const salesToken = await apiToken("xiaoshou");
    const sales = await apiGet<{ userid: string }>(salesToken, "/account/profile");
    const unreadablePurchase = await fetch(`${API_BASE}/admin/purchase-orders/${service.purchase_order_id}`, {
      headers: { Authorization: `Bearer ${salesToken}` },
      signal: AbortSignal.timeout(15000),
    });
    expect(unreadablePurchase.status).toBe(403);
    const nextCandidates = await apiGet<Array<{ user_id: string }>>(
      managerToken,
      `/admin/work-items/${before.id}/reassign-candidates`,
    );
    expect(nextCandidates.some((row) => row.user_id === sales.userid)).toBe(false);
    const deniedReturn = await command(managerToken, before.id, {
      expected_task_version: after.task_version,
      target_user_id: sales.userid,
      reason: "E2E 不具备服务履约资格的销售岗位不得接收待办",
      idempotency_key: randomUUID(),
    });
    expect(deniedReturn.status).toBe(403);
    expect(deniedReturn.parsed.errorMessage).toMatch(/读取资格|权限|范围/);
    expect(await apiGet<Task>(managerToken, `/admin/work-items/${before.id}`)).toMatchObject({
      owner_user_id: target!.user_id,
      task_version: after.task_version,
    });
    const keptPurchase = await apiGet<PurchaseIdentity>(
      managerToken,
      `/admin/purchase-orders/${service.purchase_order_id}`,
    );
    expect(keptPurchase.owner_user_id).toBe(target!.user_id);
    expect(keptPurchase.status).toBe(purchaseBefore.status);
    expect(keptPurchase.current_revision_id).toBe(purchaseBefore.current_revision_id);
    return { taskId: before.id, ownerUserId: target!.user_id };
  });
}
