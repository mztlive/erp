import { chromium, request } from "@playwright/test"
const API = "http://127.0.0.1:10001"
const ctxApi = await request.newContext()
const login = await ctxApi.post(`${API}/login`, { data: { account: "admin", password: "123456", account_kind: "admin" } })
const token = (await login.json()).data.token
const H = { Authorization: `Bearer ${token}` }

// 造一个带通配权限的测试角色
const created = await ctxApi.post(`${API}/admin/roles`, { headers: H, data: { name: "临时测试角色", permissions: ["customer:list", "customer:detail", "contract:*"] } })
console.log("create role:", created.status())
const roles = (await (await ctxApi.get(`${API}/admin/roles`, { headers: H })).json()).data
const role = roles.find((r) => r.name === "临时测试角色")
console.log("role:", role.id, role.permissions)

const browser = await chromium.launch({ headless: true })
const bctx = await browser.newContext({ viewport: { width: 1600, height: 1000 } })
const page = await bctx.newPage()
await page.goto("http://localhost:3000/login")
await page.getByPlaceholder("请输入账号").fill("admin")
await page.getByPlaceholder("请输入密码").fill("123456")
await page.getByRole("button", { name: "登录" }).click()
await page.waitForURL((u) => !u.pathname.includes("/login"), { timeout: 30000 })
await page.waitForTimeout(1200)
await page.goto(`http://localhost:3000/system/roles/${role.id}/edit`)
await page.waitForTimeout(5000)
console.log("bar:", (await page.locator("text=/已选 \\d+ 项/").first().innerText().catch(()=>"nf")))
// 勾选「客户 · 新建」
await page.getByRole("searchbox", { name: "搜索权限" }).fill("客户")
await page.waitForTimeout(1200)
const cell = page.getByRole("checkbox", { name: "客户 · 新建" }).first()
await cell.click()
await page.waitForTimeout(600)
console.log("after toggle:", await page.locator("text=/已选 \\d+ 项/").first().innerText().catch(()=>"nf"))
await page.getByRole("button", { name: "保存" }).click()
await page.waitForTimeout(3000)
console.log("url after save:", page.url())

const after = (await (await ctxApi.get(`${API}/admin/roles`, { headers: H })).json()).data.find((r) => r.id === role.id)
console.log("after save permissions:", after.permissions.sort())
await ctxApi.delete(`${API}/admin/roles/${role.id}`, { headers: H })
console.log("cleanup done")
await browser.close()
