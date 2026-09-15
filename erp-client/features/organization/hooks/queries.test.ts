import { expect, it } from "vitest"

import { dataScopeKeys, organizationKeys } from "./queries"
import type { DataScopeUrlState } from "@/features/organization/types"

it("Query key 包含范围筛选，与 URL 状态一致", () => {
    const url: DataScopeUrlState = {
        q: "销售",
        resource: "sales_order",
        action: "list",
        subjectType: "role",
        subjectId: "role-sales",
        scopeType: "company",
    }
    expect(dataScopeKeys.list(url)).toEqual([
        "data-scopes",
        "list",
        url,
    ])
    expect(organizationKeys.state()).toEqual(["organization", "state"])
})
