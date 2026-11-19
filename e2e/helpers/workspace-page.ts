import type { Page } from "@playwright/test"

import { FRONTEND_BASE_URL } from "./env"

const BUSINESS_PATH = /^\/(workspace|sales|procurement|master-data|inventory|fulfillment|finance|system|analytics|supplier-api|governance)(\/|$)/
const ACTIVE_FRAME = 'iframe[data-workspace-active="true"]'

/** ERP business content is in the active tab; login and public pages have their own document. */
export function workPage(page: Page) {
    const url = new URL(page.url())
    return url.origin === new URL(FRONTEND_BASE_URL).origin && BUSINESS_PATH.test(url.pathname)
        ? page.frameLocator(ACTIVE_FRAME)
        : page
}

export function businessDocument(page: Page) {
    return workPage(page).locator("html")
}
