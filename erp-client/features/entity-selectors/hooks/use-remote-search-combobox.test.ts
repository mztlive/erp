import { renderHook } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import {
    useRemoteSearchCombobox,
    type RemoteSearchListQuery,
    type RemoteSearchSelectedQuery,
} from "@/features/entity-selectors/hooks/use-remote-search-combobox"

type Item = Readonly<{ id: string; name: string }>

const item = (id: string): Item => ({ id, name: `item-${id}` })

function listQuery(
    overrides: Partial<RemoteSearchListQuery<Item>> = {},
): RemoteSearchListQuery<Item> {
    return {
        data: undefined,
        isFetching: false,
        isError: false,
        error: null,
        ...overrides,
    }
}

function selectedQuery(
    overrides: Partial<RemoteSearchSelectedQuery<Item>> = {},
): RemoteSearchSelectedQuery<Item> {
    return {
        data: undefined,
        isFetching: false,
        ...overrides,
    }
}

describe("useRemoteSearchCombobox", () => {
    it("merges the selected item into the list head when missing", () => {
        const { result } = renderHook(() =>
            useRemoteSearchCombobox({
                list: listQuery({ data: [item("a"), item("b")] }),
                selectedItem: item("c"),
                idOf: (it) => it.id,
                fallbackError: "加载失败",
            }),
        )
        expect(result.current.rows).toEqual([
            item("c"),
            item("a"),
            item("b"),
        ])
    })

    it("keeps the list unchanged when the selected item is already present", () => {
        const rows = [item("a"), item("b")]
        const { result } = renderHook(() =>
            useRemoteSearchCombobox({
                list: listQuery({ data: rows }),
                selectedItem: item("b"),
                idOf: (it) => it.id,
                fallbackError: "加载失败",
            }),
        )
        expect(result.current.rows).toEqual(rows)
    })

    it("falls back to the selected query data and empty list without selection", () => {
        const withDetail = renderHook(() =>
            useRemoteSearchCombobox({
                list: listQuery({ data: [item("a")] }),
                selected: selectedQuery({ data: item("b") }),
                idOf: (it) => it.id,
                fallbackError: "加载失败",
            }),
        )
        expect(withDetail.result.current.rows).toEqual([
            item("b"),
            item("a"),
        ])

        const empty = renderHook(() =>
            useRemoteSearchCombobox({
                list: listQuery(),
                idOf: (it) => it.id,
                fallbackError: "加载失败",
            }),
        )
        expect(empty.result.current.rows).toEqual([])
    })

    it("composes loading from list, selected and extra loading", () => {
        const idle = renderHook(() =>
            useRemoteSearchCombobox({
                list: listQuery(),
                selected: selectedQuery(),
                idOf: (it) => it.id,
                fallbackError: "加载失败",
            }),
        )
        expect(idle.result.current.loading).toBe(false)

        const listFetching = renderHook(() =>
            useRemoteSearchCombobox({
                list: listQuery({ isFetching: true }),
                idOf: (it) => it.id,
                fallbackError: "加载失败",
            }),
        )
        expect(listFetching.result.current.loading).toBe(true)

        const detailFetching = renderHook(() =>
            useRemoteSearchCombobox({
                list: listQuery(),
                selected: selectedQuery({ isFetching: true }),
                idOf: (it) => it.id,
                fallbackError: "加载失败",
            }),
        )
        expect(detailFetching.result.current.loading).toBe(true)

        const detailFetchingWithRow = renderHook(() =>
            useRemoteSearchCombobox({
                list: listQuery({ data: [item("a")] }),
                selected: selectedQuery({ data: item("a"), isFetching: true }),
                idOf: (it) => it.id,
                fallbackError: "加载失败",
            }),
        )
        expect(detailFetchingWithRow.result.current.loading).toBe(false)

        const extraLoading = renderHook(() =>
            useRemoteSearchCombobox({
                list: listQuery(),
                idOf: (it) => it.id,
                fallbackError: "加载失败",
                extraLoading: true,
            }),
        )
        expect(extraLoading.result.current.loading).toBe(true)
    })

    it("surfaces the list error message and the fallback when the error is empty", () => {
        const withMessage = renderHook(() =>
            useRemoteSearchCombobox({
                list: listQuery({ isError: true, error: new Error("boom") }),
                idOf: (it) => it.id,
                fallbackError: "加载失败，请重试",
            }),
        )
        expect(withMessage.result.current.emptyLabel).toBe("boom")

        const withoutMessage = renderHook(() =>
            useRemoteSearchCombobox({
                list: listQuery({ isError: true }),
                idOf: (it) => it.id,
                fallbackError: "加载失败，请重试",
            }),
        )
        expect(withoutMessage.result.current.emptyLabel).toBe("加载失败，请重试")
    })

    it("keeps the caller empty label while the list is healthy", () => {
        const custom = renderHook(() =>
            useRemoteSearchCombobox({
                list: listQuery({ data: [item("a")] }),
                idOf: (it) => it.id,
                emptyLabel: "没有符合条件的记录",
                fallbackError: "加载失败",
            }),
        )
        expect(custom.result.current.emptyLabel).toBe("没有符合条件的记录")

        const none = renderHook(() =>
            useRemoteSearchCombobox({
                list: listQuery({ data: [item("a")] }),
                idOf: (it) => it.id,
                fallbackError: "加载失败",
            }),
        )
        expect(none.result.current.emptyLabel).toBeUndefined()
    })
})
