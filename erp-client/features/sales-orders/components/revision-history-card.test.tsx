import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vitest"

import { RevisionHistoryCard } from "./revision-history-card"

afterEach(() => {
    cleanup()
})

describe("RevisionHistoryCard", () => {
    it("does not present the optimistic lock as a formal sales version", () => {
        render(
            <RevisionHistoryCard
                revisions={[]}
                currentVersion={null}
                contractRevisionLabel="HT-2026-001@v2"
            />,
        )

        expect(screen.getByText("尚未生效")).toBeTruthy()
        expect(screen.getByText("销售单尚未生效，暂无正式版本。")).toBeTruthy()
        expect(screen.queryByText(/当前 v\d+/)).toBeNull()
        expect(screen.queryByText("暂无历史版本")).toBeNull()
    })
})
