import { QuickPreviewSheet } from "@/components/business"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { DATE_BASIS_LABEL } from "../types"

export interface CardBusinessBasisSheetProps {
    open: boolean
    onOpenChange: (open: boolean) => void
}

/** 口径说明 Sheet：税额、成本覆盖与完成条件。 */
export function CardBusinessBasisSheet({
    open,
    onOpenChange,
}: CardBusinessBasisSheetProps) {
    return (
        <QuickPreviewSheet
            id="card-contracts-analytics-basis-sheet"
            open={open}
            onOpenChange={onOpenChange}
            title="卡券经营口径说明"
            description="税额、成本覆盖与完成条件"
        >
            <div className="space-y-4 text-sm">
                <DescriptionList columns="one">
                    <DescriptionItem>
                        <DescriptionTerm>含税指标</DescriptionTerm>
                        <DescriptionDetails>
                            卡券销售金额、可消费总额度、累计消费、未消费余额、未履约余额均为含税。
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>不含税指标</DescriptionTerm>
                        <DescriptionDetails>
                            实际消费成本、消费毛差、当前经营贡献、最终经营盈亏均为不含税。进项税率不被销项税率替代。
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>成本三分法</DescriptionTerm>
                        <DescriptionDetails>
                            实际成本计入利润；标准成本按消费时点的有效供给价估算；无成本仅计入消费额与覆盖率，不显示为零成本，也不计入利润。
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>当前贡献 vs 最终利润</DescriptionTerm>
                        <DescriptionDetails>
                            当前经营贡献不是最终利润；须同屏展示未履约余额。履约期限未到期范围不展示最终利润。
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>微信支付边界</DescriptionTerm>
                        <DescriptionDetails>
                            微信支付消费与成本不进入企业卡券指标；仍走供应商结算。
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>日期口径</DescriptionTerm>
                        <DescriptionDetails>
                            {Object.values(DATE_BASIS_LABEL).join("；")}
                            。未配置默认口径时须显式选择，不会自动采用本月或消费发生日。
                        </DescriptionDetails>
                    </DescriptionItem>
                </DescriptionList>
            </div>
        </QuickPreviewSheet>
    )
}
