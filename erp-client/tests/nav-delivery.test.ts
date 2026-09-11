import { describe, expect, it } from "vitest"

import {
    launchControlPoint,
    launchNavDelivery,
    NAV_DELIVERY_TARGETS,
    quadraticBezier,
    rectCenter,
    subscribeNavDelivery,
} from "@/lib/nav-delivery"

describe("nav delivery geometry", () => {
    it("uses the rectangle center as the origin", () => {
        const rect = {
            left: 10,
            top: 20,
            width: 40,
            height: 10,
        } as DOMRect
        expect(rectCenter(rect)).toEqual({ x: 30, y: 25 })
    })

    it("arcs above the shorter of origin and target", () => {
        expect(
            launchControlPoint({ x: 400, y: 300 }, { x: 80, y: 120 }),
        ).toEqual({
            x: 400 + (80 - 400) * 0.35,
            y: 0,
        })
    })

    it("starts, midpoints and ends on the quadratic curve", () => {
        expect(quadraticBezier(0, 50, 100, 0)).toBe(0)
        expect(quadraticBezier(0, 50, 100, 1)).toBe(100)
        expect(quadraticBezier(0, 50, 100, 0.5)).toBe(50)
    })
})

describe("launchNavDelivery", () => {
    it("notifies subscribers with the kind and element center", () => {
        const seen: { kind: string; x: number; y: number }[] = []
        const unsubscribe = subscribeNavDelivery(({ kind, origin }) => {
            seen.push({ kind, ...origin })
        })
        const element = {
            getBoundingClientRect: () =>
                ({ left: 0, top: 0, width: 20, height: 10 }) as DOMRect,
        } as unknown as Element
        launchNavDelivery("background-task", element)
        unsubscribe()
        launchNavDelivery("background-task", element)
        expect(seen).toEqual([{ kind: "background-task", x: 10, y: 5 }])
    })

    it("keeps sidebar entry ids aligned with nav hrefs", () => {
        expect(NAV_DELIVERY_TARGETS).toEqual({
            "selection-booklet": "sales-selection",
            "background-task": "governance-background-jobs",
        })
    })
})
