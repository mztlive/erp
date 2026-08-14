import { describe, expect, it } from "vitest"

import { mergeSelected } from "@/features/entity-selectors/lib/merge-selected"

type Item = { id: string; label: string }

const item = (id: string): Item => ({ id, label: `item-${id}` })

const idOf = (it: Item) => it.id

describe("mergeSelected", () => {
    it("returns rows as-is when nothing is selected", () => {
        const rows = [item("a"), item("b")]
        expect(mergeSelected(rows, undefined, idOf)).toEqual(rows)
        expect(mergeSelected(rows, null, idOf)).toEqual(rows)
    })

    it("returns an empty list when rows are missing", () => {
        expect(mergeSelected(undefined, undefined, idOf)).toEqual([])
        expect(mergeSelected(undefined, item("a"), idOf)).toEqual([
            item("a"),
        ])
    })

    it("prepends the selected item when it is not in the list", () => {
        const rows = [item("a"), item("b")]
        expect(mergeSelected(rows, item("c"), idOf)).toEqual([
            item("c"),
            item("a"),
            item("b"),
        ])
    })

    it("does not duplicate the selected item when already present", () => {
        const rows = [item("a"), item("b")]
        expect(mergeSelected(rows, item("b"), idOf)).toEqual(rows)
    })

    it("matches by the given id selector, not object identity", () => {
        const rows = [item("a"), item("b")]
        expect(
            mergeSelected(
                rows,
                { id: "a", label: "another-shape" },
                idOf,
            ),
        ).toEqual(rows)
    })
})
