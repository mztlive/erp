import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { BookDetailPage } from "./book-detail-page"
import type { SelectionBookDetail } from "../types"

const mocks = vi.hoisted(() => ({
    detailQuery: vi.fn(),
    operations: {
        prepare: { mutateAsync: vi.fn(), isPending: false },
        regenerate: { mutateAsync: vi.fn(), mutate: vi.fn(), isPending: false },
        reprepare: { mutateAsync: vi.fn(), isPending: false },
        publish: { mutateAsync: vi.fn(), isPending: false },
        replaceLink: { mutateAsync: vi.fn(), isPending: false },
        close: { mutateAsync: vi.fn(), isPending: false },
        revoke: { mutateAsync: vi.fn(), isPending: false },
        void: { mutateAsync: vi.fn(), isPending: false },
        deleteItem: { mutateAsync: vi.fn(), isPending: false },
        copyLink: { mutateAsync: vi.fn(), isPending: false, data: null },
    },
}))

vi.mock("../hooks/queries", () => ({
    useBookDetail: (bookId: string) => mocks.detailQuery(bookId),
    useBookOperations: () => mocks.operations,
}))

const baseDetail: SelectionBookDetail = {
    id: "book-001",
    book_id: "book-001",
    version: 2,
    customer_id: "cust-001",
    customer_name: "测试客户公司",
    sales_owner_user_id: "user-001",
    sales_owner_name: "张三",
    business_org_unit_id: "org-001",
    form: "PACKAGE",
    selection_form: "PACKAGE",
    submit_mode: "BY_QUANTITY",
    status: "PENDING_PUBLISH",
    pool_source_kind: "FILTER",
    source_kind: "FILTER",
    display_count: 2,
    removed_count: 0,
    missing_image_count: 1,
    link_revoked: false,
    tiers: [
        {
            tier_id: "tier-100",
            name: "100元档",
            target_amount: "100.00",
            tolerance: "10.00",
            expected_count: 2,
            sku_count: 3,
        },
    ],
    tier_reports: [
        {
            tier_id: "tier-100",
            expected_count: 2,
            actual_count: 2,
            stop_reason: "ENOUGH",
            stop_label: "已达目标套数",
            image_failures: 0,
        },
    ],
    items: [
        {
            id: "item-1",
            item_id: "item-1",
            kind: "PACKAGE",
            name: "精选套装A",
            price: "99.00",
            price_gross: "99.00",
            tier_id: "tier-100",
            tier_name: "100元档",
            removed: false,
            missing_image: false,
            specification: [],
            spec_label: "3件装",
            members: [
                {
                    sku_id: "sku-1",
                    name: "商品一",
                    specification: [],
                    unit: "件",
                    price: "33.00",
                },
            ],
        },
        {
            id: "item-2",
            item_id: "item-2",
            kind: "PACKAGE",
            name: "精选套装B",
            price: "105.00",
            price_gross: "105.00",
            tier_id: "tier-100",
            tier_name: "100元档",
            removed: false,
            missing_image: true,
            specification: [],
            spec_label: "3件装",
            members: [],
        },
    ],
}

const renderPage = (detail: SelectionBookDetail) => {
    mocks.detailQuery.mockReturnValue({
        data: detail,
        isPending: false,
        isError: false,
    })
    return render(
        <QueryClientProvider
            client={
                new QueryClient({
                    defaultOptions: { queries: { retry: false } },
                })
            }
        >
            <BookDetailPage bookId={detail.id} />
        </QueryClientProvider>,
    )
}

beforeEach(() => {
    vi.clearAllMocks()
})

afterEach(cleanup)

it("待发布态：渲染流程向导、侧边栏档案、陈列画廊与删减操作", () => {
    renderPage(baseDetail)

    // 1. 验证流程向导
    expect(screen.getByText("陈列已就绪，请核对并删减商品")).toBeTruthy()

    // 2. 验证侧边栏档案与指标
    expect(screen.getByText("选品册档案")).toBeTruthy()
    expect(screen.getByText("陈列指标")).toBeTruthy()
    expect(screen.getByText("有效陈列")).toBeTruthy()
    expect(screen.getByText("档位目标与达成")).toBeTruthy()

    // 3. 验证主画布陈列项与缺图角标
    expect(screen.getByText("精选套装A")).toBeTruthy()
    expect(screen.getByText("精选套装B")).toBeTruthy()
    expect(screen.getByText("缺图")).toBeTruthy()

    // 4. 验证删除陈列项动作
    const deleteBtns = screen.getAllByRole("button", {
        name: /删除该项/,
    })
    expect(deleteBtns.length).toBe(2)
    fireEvent.click(deleteBtns[0]!)
    expect(mocks.operations.deleteItem.mutateAsync).toHaveBeenCalledWith({
        bookId: "book-001",
        itemId: "item-1",
        expected_version: 2,
    })
})

it("已发布态：展示发布成功向导与客户链接卡片", () => {
    renderPage({
        ...baseDetail,
        status: "PUBLISHED",
        public_url: "https://example.com/s/test-token",
    })

    expect(screen.getByText("选品册已发布，客户专属链接生效中")).toBeTruthy()
    expect(screen.getAllByText("客户选品链接").length).toBeGreaterThan(0)
    expect(
        screen.getByDisplayValue("https://example.com/s/test-token"),
    ).toBeTruthy()
})

it("已提交态：展示方案生成向导与销售方案跳转", () => {
    renderPage({
        ...baseDetail,
        status: "SUBMITTED",
        proposal_id: "prop-123",
        proposal_no: "SP-2026-0001",
    })

    expect(screen.getByText("客户已完成选品并提交方案")).toBeTruthy()
    expect(screen.getAllByText(/SP-2026-0001/).length).toBeGreaterThan(0)
})
