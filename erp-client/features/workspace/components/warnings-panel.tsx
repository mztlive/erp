"use client"

import Link from "next/link"
import { ArrowRightIcon, Clock3Icon, TriangleAlertIcon } from "lucide-react"

import { surfacePanelClassName } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { buildWarningHref } from "@/features/workspace/lib/destination"
import type { TodayWorkspaceView } from "@/features/workspace/types"
import { goToWorkspaceLabel } from "@/lib/ui-text"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function WarningsPanel({
    warnings,
}: {
    warnings: TodayWorkspaceView["warnings"]
}) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="rounded-t-lg border-b border-grid">
                <CardTitle>需要关注的预警</CardTitle>
                <CardDescription>只显示需要你关注的异常</CardDescription>
            </CardHeader>
            <CardContent className="space-y-2">
                {warnings.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        当前没有需要关注的预警。
                    </p>
                ) : (
                    warnings.map((warning) => (
                        <Alert
                            key={warning.warningId}
                            variant={
                                warning.severity === "destructive"
                                    ? "destructive"
                                    : warning.severity === "warning"
                                      ? "warning"
                                      : "default"
                            }
                        >
                            {warning.severity === "destructive" ? (
                                <TriangleAlertIcon aria-hidden="true" />
                            ) : (
                                <Clock3Icon aria-hidden="true" />
                            )}
                            <AlertTitle>{warning.title}</AlertTitle>
                            <AlertDescription className="flex flex-col gap-2">
                                <span>{warning.description}</span>
                                <Button
                                    id={`workspace-warning-${toAutomationIdSegment(warning.warningId)}-open`}
                                    size="xs"
                                    variant="outline"
                                    className="w-fit"
                                    render={
                                        <Link
                                            id={`workspace-warning-${toAutomationIdSegment(warning.warningId)}-open`}
                                            href={buildWarningHref(warning)}
                                        />
                                    }
                                >
                                    {goToWorkspaceLabel(
                                        warning.destinationWorkspaceId,
                                    )}
                                    <ArrowRightIcon
                                        data-icon="inline-end"
                                        aria-hidden="true"
                                    />
                                </Button>
                            </AlertDescription>
                        </Alert>
                    ))
                )}
            </CardContent>
        </Card>
    )
}
