import { expect, test, type Page } from "@playwright/test"

import { apiToken } from "./api"
import { loginViaUi } from "./login"
import { chooseOption } from "./ui"

test.use({ baseURL: "https://e2e-helper.invalid" })

test("选择所属下拉的目标项一次，忽略其他浮层和退场动画", async ({ page }) => {
    await page.setContent(`
        <input id="supplier" role="combobox" aria-expanded="false">
        <div data-slot="combobox-content" data-open>
            <div role="listbox" id="other-list">
                <div role="option" data-slot="combobox-item" onclick="window.wrong = true">目标供应商</div>
            </div>
        </div>
        <div data-slot="combobox-content" data-closed data-ending-style>
            <div role="option" data-slot="combobox-item">目标供应商</div>
        </div>
        <div id="owned-popup" data-slot="combobox-content" hidden>
            <div role="listbox" id="owned-list">
                <div id="supplier-option-target" role="option" data-slot="combobox-item">目标供应商</div>
            </div>
        </div>
        <script>
            window.selections = 0;
            const input = document.getElementById('supplier');
            const popup = document.getElementById('owned-popup');
            input.addEventListener('click', () => {
                input.setAttribute('aria-expanded', 'true');
                input.setAttribute('aria-controls', 'owned-list');
                popup.hidden = false;
                popup.setAttribute('data-open', '');
            });
            document.getElementById('supplier-option-target').addEventListener('click', () => {
                window.selections += 1;
                input.value = '目标供应商';
                input.setAttribute('aria-expanded', 'false');
                popup.removeAttribute('data-open');
                popup.setAttribute('data-closed', '');
                popup.setAttribute('data-ending-style', '');
                setTimeout(() => popup.remove(), 100);
            });
        </script>
    `)
    const started = Date.now()
    await chooseOption(page, page.locator("#supplier"), /目标供应商/, "目标")
    await expect(page.locator("#supplier")).toHaveValue("目标供应商")
    expect(await page.evaluate(() => (window as unknown as { selections: number }).selections)).toBe(1)
    expect(await page.evaluate(() => (window as unknown as { wrong?: boolean }).wrong)).toBeUndefined()
    expect(Date.now() - started).toBeLessThan(3_000)
})

type LoginReply = {
    status: number
    body: object
    retryAfter?: string
}

/** 仅路由模拟登录页和 HTTP 响应；不连接应用或数据库。 */
async function mockLogin(page: Page, replies: readonly LoginReply[]): Promise<() => number> {
    let attempts = 0
    await page.route("https://e2e-helper.invalid/login", async (route) => {
        if (route.request().method() === "POST") {
            const reply = replies[Math.min(attempts++, replies.length - 1)]
            await route.fulfill({
                status: reply.status,
                contentType: "application/json",
                headers: reply.retryAfter ? { "Retry-After": reply.retryAfter } : {},
                body: JSON.stringify(reply.body),
            })
            return
        }
        await route.fulfill({
            contentType: "text/html; charset=utf-8",
            body: `
                <form>
                    <input id="governance-auth-login-account">
                    <input id="governance-auth-login-password" type="password">
                    <button id="governance-auth-login-submit" type="submit">登录</button>
                </form>
                <script>
                    document.querySelector('form').addEventListener('submit', async (event) => {
                        event.preventDefault();
                        const response = await fetch('/login', { method: 'POST' });
                        const result = await response.json();
                        if (response.ok && result.success !== false) {
                            history.replaceState(null, '', '/workspace');
                            document.body.innerHTML = '<h1>我的工作台</h1>';
                        }
                    });
                </script>
            `,
        })
    })
    return () => attempts
}

test("UI 登录失败立即使用响应详情，不先耗尽 URL 超时", async ({ page }) => {
    const attempts = await mockLogin(page, [{
        status: 401,
        body: { success: false, errorMessage: "用户名或密码错误" },
    }])
    const started = Date.now()
    await expect(loginViaUi(page, "helper-invalid-user")).rejects.toThrow("用户名或密码错误")
    expect(attempts()).toBe(1)
    expect(Date.now() - started).toBeLessThan(3_000)
})

test("UI 登录按 Retry-After 重试，并复用服务器返回的实际 JWT", async ({ page }) => {
    const attempts = await mockLogin(page, [
        { status: 429, retryAfter: "1", body: { success: false, errorMessage: "请求过于频繁" } },
        { status: 200, body: { success: true, data: { token: "helper-response-jwt" } } },
    ])
    const started = Date.now()
    await loginViaUi(page, { account: "helper-rate-limited-user", password: "helper-password" })
    expect(attempts()).toBe(2)
    expect(Date.now() - started).toBeGreaterThanOrEqual(1_000)
    expect(Date.now() - started).toBeLessThan(4_000)
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible()
    await expect(apiToken({ account: "helper-rate-limited-user", password: "helper-password" }))
        .resolves.toBe("helper-response-jwt")
})
