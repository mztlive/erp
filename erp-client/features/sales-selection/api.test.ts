import { beforeEach, expect, it, vi } from "vitest"

import { apiGet, apiPost } from "@/lib/api"
import { fetchPublicPage, savePublicSession, submitPublicSession } from "./api"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
    getApiBaseUrl: () => "http://localhost:10001",
}))

beforeEach(() => vi.resetAllMocks())

it("打开、保存和提交公开选品兼容实际接口省略 notices", async () => {
    const selecting = {
        kind: "SELECTING",
        items: [],
        choices: [],
        session_version: 1,
    }
    const receipt = {
        kind: "RECEIPT",
        items: [],
        choices: [],
        receipt: { proposal_no: "SEL-1" },
    }
    vi.mocked(apiGet).mockResolvedValue(selecting)
    vi.mocked(apiPost)
        .mockResolvedValueOnce(selecting)
        .mockResolvedValueOnce(receipt)

    expect(await fetchPublicPage("public-token")).toEqual({
        ...selecting,
        notices: [],
    })
    expect(
        await savePublicSession("public-token", {
            idempotencyKey: "save-1",
            expectedSessionVersion: 1,
            choices: [],
        }),
    ).toEqual({ ...selecting, notices: [] })
    expect(
        await submitPublicSession("public-token", {
            idempotencyKey: "submit-1",
            expectedSessionVersion: 1,
        }),
    ).toEqual({ ...receipt, notices: [] })
    expect(apiPost).toHaveBeenLastCalledWith(
        "/public/selection/public-token/submit",
        { idempotency_key: "submit-1", expected_session_version: 1 },
    )
})

it("保留后端提供的公开提示，空值提示与结束态可安全展示", async () => {
    vi.mocked(apiGet)
        .mockResolvedValueOnce({
            kind: "SELECTING",
            items: [],
            choices: [],
            notices: ["请核对选择"],
        })
        .mockResolvedValueOnce({
            kind: "ENDED",
            items: [],
            choices: [],
            notices: null,
        })
    expect((await fetchPublicPage("current")).notices).toEqual(["请核对选择"])
    expect((await fetchPublicPage("ended")).notices).toEqual([])
})
