import { ArrowLeftIcon, ArrowRightIcon } from "lucide-react"

import { Button } from "@/components/ui/button"

export type TaskNavigationProps = Readonly<{
    previousDisabled: boolean
    nextDisabled: boolean
    onPrevious: () => void
    onNext: () => void
}>

export function TaskNavigation({
    previousDisabled,
    nextDisabled,
    onPrevious,
    onNext,
}: TaskNavigationProps) {
    return (
        <div className="flex justify-between">
            <Button
                type="button"
                variant="ghost"
                disabled={previousDisabled}
                onClick={onPrevious}
            >
                <ArrowLeftIcon aria-hidden="true" />
                上一项
            </Button>
            <Button
                type="button"
                variant="ghost"
                disabled={nextDisabled}
                onClick={onNext}
            >
                下一项
                <ArrowRightIcon aria-hidden="true" />
            </Button>
        </div>
    )
}
