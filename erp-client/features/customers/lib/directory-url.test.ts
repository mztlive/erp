import { expect, it } from "vitest"
import { writeDirectoryUrl } from "./directory-url"

it("客户组织、人员、范围和分页进入同一 URL", () => {
    expect(
        writeDirectoryUrl("/sales/customers", {
            scope: "all_authorized",
            status: "disabled",
            q: "华",
            sort: "business",
            dir: "desc",
            page: 2,
            ownerUserIds: "user-2",
            orgUnitIds: "org-1,org-2",
            includeDescendants: true,
        }),
    ).toBe(
        "/sales/customers?ownerUserIds=user-2&orgUnitIds=org-1%2Corg-2&includeDescendants=true&scope=all_authorized&status=disabled&q=%E5%8D%8E&page=2",
    )
})

it("默认范围与未勾选下级不写入 URL", () => {
    expect(
        writeDirectoryUrl("/sales/customers", {
            scope: "mine",
            status: "active",
            q: "",
            sort: "business",
            dir: "desc",
            page: 1,
        }),
    ).toBe("/sales/customers")
})
