"use client"

import { PaperDocument } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import type { ContractCenterView } from "@/features/contracts/types"

type PaperClauseRow = {
  id: string
  clause: string
  content: string
}

type ContractPaperDialogProps = {
  contract: ContractCenterView | null
  open: boolean
  onOpenChange: (open: boolean) => void
}

/**
 * 纸质阅读：宽 Dialog + PaperDocument，与 detail 半屏分离。
 */
export function ContractPaperDialog({
  contract,
  open,
  onOpenChange,
}: ContractPaperDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="flex max-h-[92vh] w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-5xl"
        showCloseButton
      >
        <DialogHeader className="shrink-0 border-b border-border px-6 py-4 text-left">
          <DialogTitle>合同纸质预览</DialogTitle>
          <DialogDescription>
            系统已确认修订的打印投影。条款、状态与签章位均由服务端返回；组件不重算或拼凑正式文本。
          </DialogDescription>
        </DialogHeader>

        <div className="min-h-0 flex-1 overflow-y-auto bg-surface-sunken px-3 py-4 sm:px-6">
          {contract ? <ContractPaperDocument contract={contract} /> : null}
        </div>

        <DialogFooter className="shrink-0 border-t border-border px-6 py-4 sm:justify-between">
          <p className="text-xs text-muted-foreground">
            {contract
              ? `${contract.contractNo} · v${contract.currentRevision.revisionNo} · ${contract.statusLabel}`
              : null}
          </p>
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
            >
              关闭
            </Button>
            <Button
              type="button"
              onClick={() => {
                window.print()
              }}
            >
              打印
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function ContractPaperDocument({
  contract,
}: {
  contract: ContractCenterView
}) {
  const rev = contract.currentRevision
  const rows: PaperClauseRow[] = [
    {
      id: "payment",
      clause: "付款条件",
      content: `${rev.paymentTermSnapshot.label}。${rev.paymentTermSnapshot.description}`,
    },
    {
      id: "invoice",
      clause: "开票要求",
      content: `${rev.invoiceRequirementSnapshot.titleType}。${rev.invoiceRequirementSnapshot.contentSummary}`,
    },
    {
      id: "terms",
      clause: "条款摘要",
      content: rev.termsSummary,
    },
    {
      id: "validity",
      clause: "有效期",
      content: `${rev.validFrom} 至 ${rev.validTo}`,
    },
  ]

  return (
    <PaperDocument<PaperClauseRow>
      issuer="某某福利科技有限公司"
      title="销售合同"
      subtitle={contract.customer.displayName}
      documentNumber={contract.contractNo}
      status={{ label: contract.statusLabel, tone: contract.statusTone }}
      version={`v${rev.revisionNo}`}
      parties={[
        {
          id: "seller",
          label: "甲方（销售方）",
          name: "某某福利科技有限公司",
          reference: "内部主体",
          fields: [
            {
              id: "owner",
              label: "业务负责人",
              value: contract.ownerLabel,
            },
          ],
        },
        {
          id: "buyer",
          label: "乙方（客户）",
          name: contract.customer.displayName,
          reference: contract.customer.reference,
          fields: [
            {
              id: "settlement",
              label: "结算主体",
              value: rev.settlementParty.displayName,
            },
            {
              id: "signed",
              label: "签订日",
              value: rev.signedAt ?? "—",
              numeric: true,
            },
          ],
        },
      ]}
      metadata={[
        {
          id: "valid",
          label: "有效期",
          value: `${rev.validFrom} ~ ${rev.validTo}`,
          numeric: true,
        },
        {
          id: "effective",
          label: "生效时间",
          value: rev.effectiveAt ?? "—",
          numeric: true,
        },
        {
          id: "payment",
          label: "付款条件",
          value: rev.paymentTermSnapshot.label,
        },
        {
          id: "invoice",
          label: "开票类型",
          value: rev.invoiceRequirementSnapshot.titleType,
        },
      ]}
      lineItemLabel="合同条款投影"
      columns={[
        {
          id: "clause",
          header: "条款",
          cell: (row) => row.clause,
        },
        {
          id: "content",
          header: "内容",
          cell: (row) => (
            <span className="whitespace-normal text-sm">{row.content}</span>
          ),
        },
      ]}
      rows={rows}
      getRowId={(row) => row.id}
      totals={[
        {
          id: "note",
          label: "金额说明",
          value: "本合同不汇总“合同金额”；金额以各销售单含税金额为准。",
        },
      ]}
      remarks={
        rev.invoiceRequirementSnapshot.remark
          ? `开票备注：${rev.invoiceRequirementSnapshot.remark}`
          : "历史销售单仍按引用当时的合同修订快照履约与结算。"
      }
      signature="（签章位由服务端打印视图定义）"
      seal="公章"
      footer={`${contract.contractNo} · 修订 v${rev.revisionNo} · 状态 ${contract.statusLabel}`}
    />
  )
}
