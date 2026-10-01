import { createServer, type Server } from "node:http"
import type { AddressInfo } from "node:net"

import { expect, test as isolatedTest, isolatedApiRouting } from "./test"

type ReceivedRequest = { url: string; method: string; authorization?: string; body: string }
type Servers = { frontend: string; source: string; target: string; received: ReceivedRequest[]; sourceRequests: string[] }

async function listen(server: Server): Promise<string> {
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve))
    return `http://127.0.0.1:${(server.address() as AddressInfo).port}`
}

async function close(server: Server): Promise<void> {
    server.closeAllConnections()
    await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()))
}

const test = isolatedTest.extend<{}, { servers: Servers }>({
    servers: [async ({}, use) => {
        const received: ReceivedRequest[] = []
        const sourceRequests: string[] = []
        const frontendServer = createServer((_request, response) => {
            response.writeHead(200, { "content-type": "text/html" })
            response.end("<!doctype html><html><body>Isolation routing test</body></html>")
        })
        const sourceServer = createServer((request, response) => {
            sourceRequests.push(request.url ?? "")
            response.writeHead(418, { "access-control-allow-origin": "*" })
            response.end("source API must not receive isolated requests")
        })
        const targetServer = createServer(async (request, response) => {
            const chunks: Buffer[] = []
            for await (const chunk of request) chunks.push(Buffer.from(chunk))
            response.setHeader("access-control-allow-origin", "*")
            response.setHeader("access-control-allow-methods", "GET,POST,OPTIONS")
            response.setHeader("access-control-allow-headers", "authorization,content-type")
            if (request.method === "OPTIONS") {
                response.writeHead(204)
                response.end()
                return
            }
            received.push({
                url: request.url ?? "",
                method: request.method ?? "",
                authorization: request.headers.authorization,
                body: Buffer.concat(chunks).toString(),
            })
            response.writeHead(201, { "content-type": "application/json" })
            response.end(JSON.stringify({ target: true, url: request.url }))
        })
        const [frontend, source, target] = await Promise.all([
            listen(frontendServer), listen(sourceServer), listen(targetServer),
        ])
        const originalEnvironment = {
            API_BASE: process.env.API_BASE,
            ERP_E2E_SOURCE_API_BASE: process.env.ERP_E2E_SOURCE_API_BASE,
            ERP_E2E_CONFIG_PATH: process.env.ERP_E2E_CONFIG_PATH,
        }
        process.env.API_BASE = target
        process.env.ERP_E2E_SOURCE_API_BASE = source
        process.env.ERP_E2E_CONFIG_PATH = "routing-self-test.toml"
        try {
            await use({ frontend, source, target, received, sourceRequests })
            expect(sourceRequests, "隔离请求不得访问原后端").toEqual([])
        } finally {
            for (const [name, value] of Object.entries(originalEnvironment)) {
                if (value === undefined) delete process.env[name]
                else process.env[name] = value
            }
            await Promise.all([close(frontendServer), close(sourceServer), close(targetServer)])
        }
    }, { scope: "worker", auto: true }],
})

test("仅显式隔离且源和目标不同的运行启用转发", () => {
    const source = "http://127.0.0.1:10001"
    const target = "http://127.0.0.1:10002"
    expect(isolatedApiRouting({ API_BASE: target })).toBeNull()
    expect(isolatedApiRouting({ ERP_E2E_CONFIG_PATH: "isolated.toml", API_BASE: source })).toBeNull()
    expect(isolatedApiRouting({ ERP_E2E_CONFIG_PATH: "isolated.toml", API_BASE: `${target}/` }))
        .toEqual({ sourceOrigin: source, targetOrigin: target })
    expect(isolatedApiRouting({ ERP_E2E_ISOLATED: "1", API_BASE: target }))
        .toEqual({ sourceOrigin: source, targetOrigin: target })
    expect(() => isolatedApiRouting({ ERP_E2E_ISOLATED: "1", API_BASE: "https://127.0.0.1:10002" }))
        .toThrow("相同协议")
})

test("默认 page 真实发送方法、授权和正文，现有路径响应监听保持有效", async ({ page, servers }) => {
    await page.goto(servers.frontend)
    const url = `${servers.source}/admin/route-check?scope=one%20two`
    const responsePromise = page.waitForResponse(response => response.url().includes("/admin/route-check?") && response.request().method() === "POST")
    const result = await page.evaluate(async (requestUrl) => {
        const response = await fetch(requestUrl, {
            method: "POST",
            headers: { authorization: "Bearer isolated-token", "content-type": "application/json" },
            body: JSON.stringify({ version: "1" }),
        })
        return response.json()
    }, url)
    const response = await responsePromise
    expect(response.status()).toBe(201)
    expect(response.url()).toBe(`${servers.target}/admin/route-check?scope=one%20two`)
    expect(response.request().url()).toBe(response.url())
    expect(result).toEqual({ target: true, url: "/admin/route-check?scope=one%20two" })
    expect(servers.received).toContainEqual({
        url: "/admin/route-check?scope=one%20two",
        method: "POST",
        authorization: "Bearer isolated-token",
        body: '{"version":"1"}',
    })
})

test("直接 browser.newContext 也在首次请求前安装转发", async ({ browser, servers }) => {
    const context = await browser.newContext({ serviceWorkers: "allow" })
    try {
        const page = await context.newPage()
        await page.goto(servers.frontend)
        const result = await page.evaluate(async (url) => (await fetch(url)).json(), `${servers.source}/admin/raw-context?page=2`)
        expect(result).toEqual({ target: true, url: "/admin/raw-context?page=2" })
        expect(servers.received).toContainEqual({
            url: "/admin/raw-context?page=2", method: "GET", authorization: undefined, body: "",
        })
    } finally {
        await context.close()
    }
})

test("页面和随后注册的 context mock 仍优先于隔离转发", async ({ page, context, servers }) => {
    await page.route(`${servers.source}/admin/page-mock`, route => route.fulfill({
        contentType: "application/json", body: '{"mock":"page"}', headers: { "access-control-allow-origin": "*" },
    }))
    await context.route(`${servers.source}/admin/context-mock`, route => route.fulfill({
        contentType: "application/json", body: '{"mock":"context"}', headers: { "access-control-allow-origin": "*" },
    }))
    await page.goto(servers.frontend)
    for (const level of ["page", "context"]) {
        const result = await page.evaluate(async (url) => (await fetch(url)).json(), `${servers.source}/admin/${level}-mock`)
        expect(result).toEqual({ mock: level })
    }
    expect(servers.received.filter(request => request.url.includes("-mock"))).toEqual([])
})
