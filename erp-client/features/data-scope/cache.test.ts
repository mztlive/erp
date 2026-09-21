import { QueryClient, QueryObserver } from "@tanstack/react-query"
import { describe, expect, it, vi } from "vitest"
import { subscribeScopeCache } from "./cache"

const profile = (version: number) => ({
    userid: "manager",
    policy_version: version,
    organization_version: 1,
    permissions: ["sales_order:list"],
})
const makeClient = () =>
    new QueryClient({
        defaultOptions: { queries: { retry: false, gcTime: Infinity } },
    })

describe("scope cache across features", () => {
    it("clears other features immediately when a report observes a newer organization version", async () => {
        const client = makeClient()
        const stop = subscribeScopeCache(client)
        client.setQueryData(["account", "profile"], profile(1))
        client.setQueryData(["sales-orders", "list"], "private prior team")
        client.setQueryData(["actual-profit-loss", "view"], {
            policyVersion: 1,
            organizationVersion: 2,
            scopeVersion: "team-b",
        })
        await vi.waitFor(() =>
            expect(
                client.getQueryData(["sales-orders", "list"]),
            ).toBeUndefined(),
        )
        expect(client.getQueryData(["actual-profit-loss", "view"])).toEqual({
            policyVersion: 1,
            organizationVersion: 2,
            scopeVersion: "team-b",
        })
        stop()
        client.clear()
    })

    it("rejects an older response after a newer policy has already been observed", () => {
        const client = makeClient()
        const stop = subscribeScopeCache(client)
        client.setQueryData(["account", "profile"], profile(2))
        client.setQueryData(["sales", "detail"], {
            policyVersion: 1,
            organizationVersion: 1,
            secret: "obsolete response",
        })
        expect(client.getQueryData(["sales", "detail"])).toBeUndefined()
        expect(client.getQueryState(["sales", "detail"])?.status).toBe("error")
        expect(client.getQueryData(["account", "profile"])).toEqual(profile(2))
        stop()
        client.clear()
    })

    it("drops sales, costs and exports when policy changes, keeping the new profile", async () => {
        const client = makeClient()
        const stop = subscribeScopeCache(client)
        client.setQueryData(["account", "profile"], profile(1))
        client.setQueryData(["sales-orders", "list"], { private: "old sales" })
        client.setQueryData(["actual-profit-loss", "cost-entries"], {
            private: "old costs",
        })
        client.setQueryData(["exports", "old"], { private: "download" })
        client.setQueryData(["account", "profile"], profile(2))
        await vi.waitFor(() =>
            expect(
                client.getQueryData(["sales-orders", "list"]),
            ).toBeUndefined(),
        )
        expect(
            client.getQueryData(["actual-profit-loss", "cost-entries"]),
        ).toBeUndefined()
        expect(client.getQueryData(["exports", "old"])).toBeUndefined()
        expect(client.getQueryData(["account", "profile"])).toEqual(profile(2))
        stop()
        client.clear()
    })
    it("clears cache when 409 carries DATA_SCOPE_CHANGED code", async () => {
        const client = makeClient()
        const stop = subscribeScopeCache(client)
        const error = Object.assign(
            new Error("数据范围已变化，请从第一页刷新"),
            {
                status: 409,
                code: "DATA_SCOPE_CHANGED",
            },
        )
        client.setQueryData(["customers", "directory"], { secret: "old" })
        client.setQueryData(["sales-orders", "list"], { secret: "old sales" })
        const observer = new QueryObserver(client, {
            queryKey: ["customers", "directory"],
            queryFn: async () => {
                throw error
            },
            enabled: false,
        })
        const unobserve = observer.subscribe(() => {})
        await observer.refetch()
        expect(observer.getCurrentResult().data).toBeUndefined()
        expect(observer.getCurrentResult().error).toBe(error)
        expect(client.getQueryData(["sales-orders", "list"])).toBeUndefined()
        unobserve()
        stop()
        client.clear()
    })
    it.each([403, 404])(
        "clears only the rejected query on %s and keeps unrelated data",
        async (status) => {
            const client = makeClient()
            const stop = subscribeScopeCache(client)
            const error = Object.assign(new Error("不可查看"), { status })
            client.setQueryData(["sales", "detail"], { secret: "old" })
            client.setQueryData(["actual-profit-loss", "view"], {
                secret: "old totals",
            })
            const observer = new QueryObserver(client, {
                queryKey: ["sales", "detail"],
                queryFn: async () => {
                    throw error
                },
                enabled: false,
            })
            const unobserve = observer.subscribe(() => {})
            await observer.refetch()
            expect(observer.getCurrentResult().data).toBeUndefined()
            expect(observer.getCurrentResult().error).toBe(error)
            expect(client.getQueryData(["actual-profit-loss", "view"])).toEqual(
                { secret: "old totals" },
            )
            unobserve()
            stop()
            client.clear()
        },
    )
    it("a late category rejection cannot replace loaded products or contracts with a permission error", async () => {
        const client = makeClient()
        const stop = subscribeScopeCache(client)
        const observers = [
            new QueryObserver(client, {
                queryKey: [
                    "master-data",
                    "list",
                    { resource: "sellable-items" },
                ],
                queryFn: async () => ["商品"],
            }),
            new QueryObserver(client, {
                queryKey: ["contracts", "list"],
                queryFn: async () => ["合同"],
            }),
        ]
        const cleanups = observers.map((observer) =>
            observer.subscribe(() => {}),
        )
        await Promise.all(observers.map((observer) => observer.refetch()))
        await client
            .fetchQuery({
                queryKey: ["master-data", "product-filter-options"],
                queryFn: async () => {
                    throw Object.assign(new Error("没有分类权限"), {
                        status: 403,
                    })
                },
            })
            .catch(() => {})
        expect(observers[0].getCurrentResult()).toMatchObject({
            status: "success",
            data: ["商品"],
            error: null,
        })
        expect(observers[1].getCurrentResult()).toMatchObject({
            status: "success",
            data: ["合同"],
            error: null,
        })
        cleanups.forEach((cleanup) => cleanup())
        stop()
        client.clear()
    })

    it("rechecks the profile on 403 and clears other data when revocation is confirmed", async () => {
        const client = makeClient()
        const stop = subscribeScopeCache(client)
        client.setQueryData(["account", "profile"], profile(1))
        client.setQueryData(["contracts", "list"], ["旧合同"])
        const fetchProfile = vi.fn(async () => profile(2))
        const observer = new QueryObserver(client, {
            queryKey: ["account", "profile"],
            queryFn: fetchProfile,
            staleTime: Infinity,
        })
        const unobserve = observer.subscribe(() => {})
        await client
            .fetchQuery({
                queryKey: ["sales", "detail"],
                queryFn: async () => {
                    throw Object.assign(new Error("撤权"), { status: 403 })
                },
            })
            .catch(() => {})
        await vi.waitFor(() =>
            expect(client.getQueryData(["contracts", "list"])).toBeUndefined(),
        )
        expect(fetchProfile).toHaveBeenCalledTimes(1)
        expect(client.getQueryData(["account", "profile"])).toEqual(profile(2))
        unobserve()
        stop()
        client.clear()
    })

    it.each([403, 404])(
        "a rejected mutation (%s) does not poison unrelated reads",
        async (status) => {
            const client = makeClient()
            const stop = subscribeScopeCache(client)
            client.setQueryData(["contracts", "list"], ["可读合同"])
            await client
                .getMutationCache()
                .build(client, {
                    mutationFn: async () => {
                        throw Object.assign(new Error("操作不可用"), { status })
                    },
                })
                .execute(undefined)
                .catch(() => {})
            expect(client.getQueryData(["contracts", "list"])).toEqual([
                "可读合同",
            ])
            stop()
            client.clear()
        },
    )

    it("cancels an outstanding old response so it cannot repopulate removed cache", async () => {
        const client = makeClient()
        const stop = subscribeScopeCache(client)
        client.setQueryData(["account", "profile"], profile(1))
        let resolve!: (value: string) => void
        const request = client
            .fetchQuery({
                queryKey: ["costs", "slow"],
                queryFn: () =>
                    new Promise<string>((done) => {
                        resolve = done
                    }),
            })
            .catch(() => undefined)
        client.setQueryData(["account", "profile"], profile(2))
        resolve("private old response")
        await request
        expect(client.getQueryData(["costs", "slow"])).toBeUndefined()
        stop()
        client.clear()
    })
    it("does not invalidate for failed writes or read-only exports", async () => {
        const client = makeClient()
        const stop = subscribeScopeCache(client)
        client.setQueryData(["sales"], "keep")
        await client
            .getMutationCache()
            .build(client, { mutationFn: async () => "csv" })
            .execute(undefined)
        expect(client.getQueryData(["sales"])).toBe("keep")
        await client
            .getMutationCache()
            .build(client, {
                meta: { affectsDataScope: true },
                mutationFn: async () => {
                    throw new Error("conflict")
                },
            })
            .execute(undefined)
            .catch(() => {})
        expect(client.getQueryData(["sales"])).toBe("keep")
        await client
            .getMutationCache()
            .build(client, {
                meta: { affectsDataScope: true },
                mutationFn: async () => "done",
            })
            .execute(undefined)
        await vi.waitFor(() =>
            expect(client.getQueryData(["sales"])).toBeUndefined(),
        )
        stop()
        client.clear()
    })
})
