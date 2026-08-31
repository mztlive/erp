import Link from "next/link"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"

function CrossEntryBanner({ returnTo }: { returnTo: string }) {
    return (
        <Alert>
            <AlertTitle>跨页面进入</AlertTitle>
            <AlertDescription>
                已按来源单据的供应商预填筛选；完成对账结算后请返回来源页。{" "}
                <Link
                    id="supplier-settlements-cross-entry-back"
                    className="underline"
                    href={returnTo}
                >
                    返回来源
                </Link>
            </AlertDescription>
        </Alert>
    )
}

export { CrossEntryBanner }
