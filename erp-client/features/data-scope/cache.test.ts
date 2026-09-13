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
    it.each([403, 404])(
        "clears active previous data on %s and removes cross-feature data",
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
            expect(
                client.getQueryData(["actual-profit-loss", "view"]),
            ).toBeUndefined()
            unobserve()
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
