import { cleanup, render, waitFor } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import { NavDeliveryOverlay } from "@/components/layout/nav-delivery-overlay"
import { launchNavDelivery } from "@/lib/nav-delivery"

afterEach(() => {
    cleanup()
    document.body.innerHTML = ""
})

function renderedChip(): HTMLElement | null {
    return document.querySelector<HTMLElement>(
        '[data-slot="nav-delivery-particle"]',
    )
}

function renderedPulse(): HTMLElement | null {
    return document.querySelector<HTMLElement>(
        '[data-slot="nav-delivery-pulse"]',
    )
}

function mountNavTarget(navId: string): HTMLElement {
    const target = document.createElement("button")
    target.dataset.workspaceNav = navId
    target.getBoundingClientRect = () =>
        ({
            left: 20,
            top: 40,
            width: 10,
            height: 10,
        }) as DOMRect
    document.body.append(target)
    return target
}

function mountOrigin(): HTMLElement {
    const origin = document.createElement("button")
    origin.getBoundingClientRect = () =>
        ({ left: 1000, top: 500, width: 40, height: 20 }) as DOMRect
    document.body.append(origin)
    return origin
}

test("投递后粒子飞向侧栏入口并把入口滚进视野", async () => {
    const target = mountNavTarget("governance-background-jobs")
    const scrollIntoView = vi.fn()
    target.scrollIntoView = scrollIntoView
    render(<NavDeliveryOverlay />)

    launchNavDelivery("background-task", mountOrigin())

    await waitFor(() => expect(renderedChip()).not.toBeNull())
    const chip = renderedChip()!
    expect(chip.querySelector("svg")).not.toBeNull()
    expect(chip.style.transform).toContain("translate(1020px, 510px)")
    expect(scrollIntoView).toHaveBeenCalledWith({ block: "nearest" })
})

test("入口不在侧栏时退化为落点闪一下", async () => {
    render(<NavDeliveryOverlay />)

    launchNavDelivery("selection-booklet", mountOrigin())

    await waitFor(() => expect(renderedPulse()).not.toBeNull())
    expect(renderedChip()).toBeNull()
})
