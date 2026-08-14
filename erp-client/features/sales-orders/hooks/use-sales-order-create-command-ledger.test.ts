import { describe, it, expect } from 'vitest'
import { renderHook } from '@testing-library/react'

import { FormalCommandKeyLedger } from '@/lib/formal-command'
import { useSalesOrderCreateCommandLedger } from './use-sales-order-create-command-ledger'

describe('useSalesOrderCreateCommandLedger', () => {
    it('keeps the same ledger instance while the scope is unchanged', () => {
        const { result, rerender } = renderHook(
            ({ scope }: { scope: string }) =>
                useSalesOrderCreateCommandLedger(scope),
            { initialProps: { scope: 'so-1' } },
        )

        const first = result.current
        expect(first).toBeInstanceOf(FormalCommandKeyLedger)

        rerender({ scope: 'so-1' })
        expect(result.current).toBe(first)
    })

    it('rebuilds the ledger when the scope changes', () => {
        const { result, rerender } = renderHook(
            ({ scope }: { scope: string }) =>
                useSalesOrderCreateCommandLedger(scope),
            { initialProps: { scope: 'so-1' } },
        )

        const first = result.current
        rerender({ scope: 'so-2' })

        expect(result.current).not.toBe(first)
        expect(result.current).toBeInstanceOf(FormalCommandKeyLedger)

        rerender({ scope: 'so-1' })
        expect(result.current).not.toBe(first)
    })

    it('prefers the provided ledger over the internal one', () => {
        const provided = new FormalCommandKeyLedger()
        const { result, rerender } = renderHook(
            ({ scope }: { scope: string }) =>
                useSalesOrderCreateCommandLedger(scope, provided),
            { initialProps: { scope: 'so-1' } },
        )

        expect(result.current).toBe(provided)
        rerender({ scope: 'so-2' })
        expect(result.current).toBe(provided)
    })
})
