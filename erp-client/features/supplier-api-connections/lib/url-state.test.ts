import { describe, expect, it } from "vitest"

import {
    buildConnectionsSearchParams,
    parseConnectionsSearchParams,
} from "@/features/supplier-api-connections/lib/url-state"

describe("parseConnectionsSearchParams", () => {
    it("returns defaults for an empty query string", () => {
        const state = parseConnectionsSearchParams(new URLSearchParams())
        expect(state.environment).toBe("PRODUCTION")
        expect(state.page).toBe(1)
        expect(state.pageSize).toBe(20)
        expect(state.section).toBe("overview")
        expect(state.connectionId).toBeUndefined()
        expect(state.status).toBeUndefined()
        expect(state.health).toBeUndefined()
        expect(state.capability).toBeUndefined()
        expect(state.catalogFreshness).toBeUndefined()
        expect(state.supplierId).toBeUndefined()
        expect(state.q).toBeUndefined()
    })

    it("normalizes the environment value to upper case", () => {
        expect(
            parseConnectionsSearchParams(
                new URLSearchParams("environment=staging"),
            ).environment,
        ).toBe("STAGING")
        expect(
            parseConnectionsSearchParams(
                new URLSearchParams("environment=production"),
            ).environment,
        ).toBe("PRODUCTION")
        expect(
            parseConnectionsSearchParams(new URLSearchParams("environment=all"))
                .environment,
        ).toBe("ALL")
    })

    it("falls back to the default for unknown environment values", () => {
        expect(
            parseConnectionsSearchParams(
                new URLSearchParams("environment=bogus"),
            ).environment,
        ).toBe("PRODUCTION")
    })

    it("clamps page and pageSize to their bounds", () => {
        const params = new URLSearchParams("page=0&pageSize=10")
        const state = parseConnectionsSearchParams(params)
        expect(state.page).toBe(1)
        expect(state.pageSize).toBe(20)

        const big = parseConnectionsSearchParams(
            new URLSearchParams("page=3.7&pageSize=999"),
        )
        expect(big.page).toBe(3)
        expect(big.pageSize).toBe(100)

        const invalid = parseConnectionsSearchParams(
            new URLSearchParams("page=abc&pageSize=xyz"),
        )
        expect(invalid.page).toBe(1)
        expect(invalid.pageSize).toBe(20)
    })

    it("reads connectionId from the primary key and its alias", () => {
        expect(
            parseConnectionsSearchParams(new URLSearchParams("connectionId=c1"))
                .connectionId,
        ).toBe("c1")
        expect(
            parseConnectionsSearchParams(new URLSearchParams("id=c2"))
                .connectionId,
        ).toBe("c2")
        // 主键优先于别名
        expect(
            parseConnectionsSearchParams(
                new URLSearchParams("id=c2&connectionId=c1"),
            ).connectionId,
        ).toBe("c1")
    })

    it("passes through filter strings and falls back on invalid sections", () => {
        const state = parseConnectionsSearchParams(
            new URLSearchParams(
                "status=ENABLED&health=FAILED,AUTH_FAILED&capability=PRICE&catalogFreshness=STALE&supplierId=s1&q=CONN",
            ),
        )
        expect(state.status).toBe("ENABLED")
        expect(state.health).toBe("FAILED,AUTH_FAILED")
        expect(state.capability).toBe("PRICE")
        expect(state.catalogFreshness).toBe("STALE")
        expect(state.supplierId).toBe("s1")
        expect(state.q).toBe("CONN")
        expect(
            parseConnectionsSearchParams(new URLSearchParams("section=bogus"))
                .section,
        ).toBe("overview")
        expect(
            parseConnectionsSearchParams(new URLSearchParams("section=health"))
                .section,
        ).toBe("health")
    })
})

describe("buildConnectionsSearchParams", () => {
    it("builds an empty string for default state", () => {
        expect(
            buildConnectionsSearchParams(
                parseConnectionsSearchParams(new URLSearchParams()),
            ),
        ).toBe("")
    })

    it("writes non-default filter and page params and trims q", () => {
        const qs = buildConnectionsSearchParams({
            environment: "ALL",
            status: "ENABLED",
            capability: "PRICE",
            supplierId: "s1",
            q: "  CONN-1  ",
            page: 2,
            pageSize: 50,
            section: "overview",
        })
        const params = new URLSearchParams(qs.slice(1))
        expect(params.get("environment")).toBe("ALL")
        expect(params.get("status")).toBe("ENABLED")
        expect(params.get("capability")).toBe("PRICE")
        expect(params.get("supplierId")).toBe("s1")
        expect(params.get("q")).toBe("CONN-1")
        expect(params.get("page")).toBe("2")
        expect(params.get("pageSize")).toBe("50")
    })

    it("writes section only when it is not overview and a connection is open", () => {
        expect(
            buildConnectionsSearchParams({
                environment: "PRODUCTION",
                page: 1,
                pageSize: 20,
                connectionId: "c1",
                section: "overview",
            }),
        ).toBe("?connectionId=c1")

        const withSection = buildConnectionsSearchParams({
            environment: "PRODUCTION",
            page: 1,
            pageSize: 20,
            connectionId: "c1",
            section: "health",
        })
        const params = new URLSearchParams(withSection.slice(1))
        expect(params.get("connectionId")).toBe("c1")
        expect(params.get("section")).toBe("health")

        // 没有 connectionId 时 section 也不写回
        expect(
            buildConnectionsSearchParams({
                environment: "PRODUCTION",
                page: 1,
                pageSize: 20,
                section: "health",
            }),
        ).toBe("")
    })

    it("round-trips a fully filtered state", () => {
        const qs = buildConnectionsSearchParams({
            environment: "STAGING",
            status: "ENABLED,FAULTED",
            health: "FAILED",
            capability: "ORDER",
            catalogFreshness: "STALE,FAILED",
            supplierId: "s9",
            q: "CONN",
            page: 3,
            pageSize: 20,
            connectionId: "c7",
            section: "audit",
        })
        const parsed = parseConnectionsSearchParams(
            new URLSearchParams(qs.slice(1)),
        )
        expect(parsed).toMatchObject({
            environment: "STAGING",
            status: "ENABLED,FAULTED",
            health: "FAILED",
            capability: "ORDER",
            catalogFreshness: "STALE,FAILED",
            supplierId: "s9",
            q: "CONN",
            page: 3,
            pageSize: 20,
            connectionId: "c7",
            section: "audit",
        })
    })
})
