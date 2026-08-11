import * as React from "react"
import { functionalUpdate, type OnChangeFn, type Updater } from "@tanstack/react-table"

type ControlledTableStateProps<T> = {
    value?: T
    defaultValue: T
    onChange?: (value: T) => void
}

export function useControlledTableState<T>({
    value,
    defaultValue,
    onChange,
}: ControlledTableStateProps<T>): [T, OnChangeFn<T>] {
    const [internalValue, setInternalValue] = React.useState(defaultValue)
    const currentValue = value ?? internalValue

    const handleChange = React.useCallback(
        (updater: Updater<T>) => {
            const nextValue = functionalUpdate(updater, currentValue)
            if (value === undefined) setInternalValue(nextValue)
            onChange?.(nextValue)
        },
        [currentValue, onChange, value],
    )

    return [currentValue, handleChange]
}
