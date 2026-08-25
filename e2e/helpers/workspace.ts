import { expect, type Locator, type Page } from "@playwright/test";

import { gotoPage } from "./ui";

/**
 * W01 我的工作台定位与审批动作。
 *
 * 与现网布局对齐：页头「我的工作台」+ 口径分段（无顶部统计数字、无 MetricStrip）；
 * 待办列表只渲染一份；桌面队列/作业左右分栏，窄屏用 Sheet。
 */

/** 唯一待办列表。桌面与窄屏共用，不再双份渲染。 */
export function workspaceTaskList(page: Page): Locator {
  return page.getByRole("list", { name: "待办列表" });
}

/** 列表内任务行按钮（aria-label 为「类型标签 + 稳定单号」）。 */
export function workspaceTaskButtons(page: Page): Locator {
  return workspaceTaskList(page).getByRole("button");
}

/** 当前可见的任务详情（桌面主从内联；窄屏打开 Sheet 后）。 */
export function workspaceTaskDetail(page: Page): Locator {
  return page.locator('section[aria-label="当前任务"]:visible');
}

export async function expectWorkspaceHome(page: Page): Promise<void> {
  await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
    timeout: 20_000,
  });
  await expect(page.getByLabel("待办筛选")).toBeVisible();
  await expect(page.getByLabel("任务类型")).toBeVisible();
  await expect(page.locator('[data-slot="metric-strip"]')).toHaveCount(0);
  await expect(page.getByText("工作台汇总")).toHaveCount(0);
  await expect(page.getByText(/早上好|下午好|晚上好/)).toHaveCount(0);
  await expect(
    page.getByLabel("待办筛选").getByText(/^\d+$/),
  ).toHaveCount(0);
}

export async function gotoWorkspace(page: Page): Promise<void> {
  await gotoPage(page, "/workspace");
  await expectWorkspaceHome(page);
}

export async function openWorkspaceTask(
  page: Page,
  task: Locator,
): Promise<void> {
  await expect(task).toBeVisible({ timeout: 30_000 });
  await task.click();
  await expect(workspaceTaskDetail(page)).toBeVisible({ timeout: 20_000 });
}

export async function approveVisibleWorkspaceTask(page: Page): Promise<void> {
  const detail = workspaceTaskDetail(page);
  await expect(detail).toBeVisible({ timeout: 20_000 });
  await detail.getByRole("button", { name: "通过", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
}

/** 隐藏会遮挡移动端业务按钮的开发调试悬浮层。 */
async function hideDevelopmentToolOverlays(page: Page): Promise<void> {
  await page.addStyleTag({
    content: ".tsqd-parent-container { display: none !important; }",
  });
}

/** 打开非审批任务，并通过固定业务按钮进入对应单据页面。 */
export async function openVisibleWorkspaceDocument(page: Page): Promise<void> {
  const detail = workspaceTaskDetail(page);
  await expect(detail).toBeVisible({ timeout: 20_000 });
  await expect(
    detail.getByRole("button", { name: "通过", exact: true }),
  ).toHaveCount(0);
  await expect(
    detail.getByRole("button", { name: "驳回", exact: true }),
  ).toHaveCount(0);
  await hideDevelopmentToolOverlays(page);
  await detail.getByRole("button", { name: "打开单据", exact: true }).click();
}

/** 打开工作台并通过列表第一行。调用方自行断言任务消失或空态。 */
export async function approveFirstWorkspaceTask(page: Page): Promise<Locator> {
  const firstTask = workspaceTaskButtons(page).first();
  await expect(firstTask).toBeVisible({ timeout: 30_000 });
  const taskId = await firstTask.getAttribute("id");
  const task = taskId
    ? page.locator(`[id=${JSON.stringify(taskId)}]`)
    : firstTask;
  await openWorkspaceTask(page, task);
  await approveVisibleWorkspaceTask(page);
  // 审批命令成功并由查询刷新移除任务后才能切号，否则整页导航会中断在途请求。
  await expect(task).toHaveCount(0, { timeout: 30_000 });
  return task;
}

/**
 * 按任务按钮 id 通过。id 形如 `work-item-${stableNumber}`，
 * 与 workspace-task-list.tsx 一致。
 */
export async function approveWorkspaceTaskByButtonId(
  page: Page,
  taskButtonId: string,
): Promise<Locator> {
  const id = taskButtonId.startsWith("#")
    ? taskButtonId.slice(1)
    : taskButtonId.startsWith("work-item-")
      ? taskButtonId
      : `work-item-${taskButtonId}`;
  const task = page.locator(`#${id}`);
  await openWorkspaceTask(page, task);
  await approveVisibleWorkspaceTask(page);
  await expect(task).toHaveCount(0, { timeout: 30_000 });
  return task;
}

/**
 * 按业务对象稳定单号/可见文案通过。列表行现在展示类型 + stableNumber，
 * 详情标题仍带对象标签；两者任一命中即可。
 */
export async function approveWorkspaceTaskByDocumentNo(
  page: Page,
  docNo: string,
): Promise<Locator> {
  const rows = workspaceTaskButtons(page);
  await expect(rows.first()).toBeVisible({ timeout: 30_000 });
  const listed = rows.filter({ hasText: docNo });
  const task = (await listed.count()) > 0 ? listed.first() : rows.first();
  await openWorkspaceTask(page, task);
  await expect(
    workspaceTaskDetail(page).getByText(new RegExp(docNo)).first(),
  ).toBeVisible({ timeout: 20_000 });
  await approveVisibleWorkspaceTask(page);
  await expect(task).toHaveCount(0, { timeout: 30_000 });
  return task;
}
