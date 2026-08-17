/** 采购确认写入：保存分行草稿与提交通过/驳回决策。 */

import { apiPost, apiPut } from "@/lib/api"

import { apiErrorCode, apiErrorMessage, isApiError } from "./errors"
import type {
    ConfirmationLineDraft,
    FormalActionResponse,
    FormalOutcome,
    RejectReasonCode,
} from "@/features/procurement-confirmation/types"

export async function saveProcurementConfirmation(input: {
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    confirmationId: string
    submissionId: string
    expectedEditVersion: number
    lines: ConfirmationLineDraft[]
    idempotencyKey: string
}): Promise<{ editVersion: number; taskVersion: string }> {
    const body = {
        work_item_id: input.workItemId,
        expected_task_version: input.expectedTaskVersion,
        expected_subject_version: input.expectedSubjectVersion,
        action: {
            confirmation_id: input.confirmationId,
            submission_id: input.submissionId,
            expected_edit_version: input.expectedEditVersion,
            lines: toConfirmationLinePayload(input.lines),
        },
        idempotency_key: input.idempotencyKey,
    }

    const detail = await apiPut<{
        edit_version: number
        task_version: string | number
    }>(
        `/admin/procurement-confirmations/${encodeURIComponent(input.confirmationId)}/lines`,
        body,
    )
    if (
        !Number.isInteger(detail.edit_version) ||
        (typeof detail.task_version !== "string" &&
            typeof detail.task_version !== "number")
    ) {
        throw new Error(
            "保存接口未返回新的 editVersion 与 taskVersion；当前处理器尚未满足强类型保存合同",
        )
    }
    return {
        editVersion: detail.edit_version,
        taskVersion: String(detail.task_version),
    }
}

export async function completeProcurementDecision(input: {
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    idempotencyKey: string
    decision:
        | {
              reviewResult: "APPROVED"
              confirmationId: string
              submissionId: string
              expectedConfirmationEditVersion: number
              salesOrderId: string
              salesOrderNo: string
              subjectHash: string
              lines: ConfirmationLineDraft[]
          }
        | {
              reviewResult: "REJECTED"
              confirmationId: string
              submissionId: string
              expectedConfirmationEditVersion: number
              salesOrderId: string
              salesOrderNo: string
              subjectHash: string
              rejectReasonCode: RejectReasonCode
              comment: string
          }
}): Promise<FormalActionResponse> {
    try {
        const data = await apiPost<{
            work_item_id: string
            work_item_status: "COMPLETED"
            business_result:
                | {
                      outcome: "APPROVED_AND_SALES_EFFECTIVE"
                      procurement_confirmation_id: string
                      sales_order_id: string
                      submission_id: string
                      sales_order_revision_id: string
                      receivable_account_id: string
                      procurement_creation_basis_id: string
                      purchase_orders?: Array<{
                          purchase_order_id: string
                          purchase_no: string
                      }>
                  }
                | {
                      outcome: "REJECTED_TO_SALES"
                      procurement_confirmation_id: string
                      sales_order_id: string
                      rejected_submission_id: string
                      workflow_action_id: string
                      next_sales_resolutions: readonly [
                          "RESUBMIT_CHANGED_TERMS",
                          "REQUEST_LOW_MARGIN_ACCEPTANCE",
                          "VOID_AFTER_REJECTION",
                      ]
                      successor_work_item_id?: string | null
                  }
        }>(
            `/admin/procurement-confirmations/${encodeURIComponent(input.decision.confirmationId)}/decisions`,
            {
                work_item_id: input.workItemId,
                expected_task_version: input.expectedTaskVersion,
                expected_subject_version: input.expectedSubjectVersion,
                decision:
                    input.decision.reviewResult === "APPROVED"
                        ? {
                              review_result: "APPROVED",
                              confirmation_id: input.decision.confirmationId,
                              submission_id: input.decision.submissionId,
                              expected_confirmation_edit_version:
                                  input.decision
                                      .expectedConfirmationEditVersion,
                              lines: toConfirmationLinePayload(
                                  input.decision.lines,
                              ),
                          }
                        : {
                              review_result: "REJECTED",
                              confirmation_id: input.decision.confirmationId,
                              submission_id: input.decision.submissionId,
                              expected_confirmation_edit_version:
                                  input.decision
                                      .expectedConfirmationEditVersion,
                              reject_reason_code:
                                  input.decision.rejectReasonCode,
                              comment: input.decision.comment,
                          },
                idempotency_key: input.idempotencyKey,
            },
        )
        if (input.decision.reviewResult === "APPROVED") {
            const business = data.business_result
            if (
                data.work_item_status !== "COMPLETED" ||
                business?.outcome !== "APPROVED_AND_SALES_EFFECTIVE" ||
                !business.procurement_creation_basis_id
            ) {
                return {
                    status: "failed",
                    code: "INCOMPLETE_FORMAL_RESULT",
                    message:
                        "任务完成记录或采购单草稿不完整；当前结果不能按成功展示",
                }
            }
            const purchaseOrders = (business.purchase_orders ?? [])
                .filter((order) => order.purchase_order_id && order.purchase_no)
                .map((order) => ({
                    purchaseOrderId: order.purchase_order_id,
                    purchaseNo: order.purchase_no,
                }))
            if (purchaseOrders.length === 0) {
                return {
                    status: "failed",
                    code: "INCOMPLETE_FORMAL_RESULT",
                    message:
                        "任务完成记录或采购单草稿不完整；当前结果不能按成功展示",
                }
            }
            const outcome: FormalOutcome = {
                kind: "APPROVED_AND_SALES_EFFECTIVE",
                procurementConfirmationId: business.procurement_confirmation_id,
                salesOrderId: business.sales_order_id,
                salesOrderNo: input.decision.salesOrderNo,
                submissionId: business.submission_id,
                subjectHash: input.decision.subjectHash,
                salesOrderRevisionId: business.sales_order_revision_id,
                receivableAccountId: business.receivable_account_id,
                procurementCreationBasisId:
                    business.procurement_creation_basis_id,
                purchaseOrders,
                reference:
                    purchaseOrders[0]?.purchaseOrderId ??
                    business.procurement_creation_basis_id,
            }
            return { status: "succeeded", outcome }
        }

        const business = data.business_result
        if (
            data.work_item_status !== "COMPLETED" ||
            business?.outcome !== "REJECTED_TO_SALES" ||
            business.successor_work_item_id != null ||
            !business.next_sales_resolutions.includes(
                "REQUEST_LOW_MARGIN_ACCEPTANCE",
            )
        ) {
            return {
                status: "failed",
                code: "INCOMPLETE_FORMAL_RESULT",
                message:
                    "驳回结果不完整，或出现了不应存在的后续任务；当前结果不能按成功展示",
            }
        }
        const outcome: FormalOutcome = {
            kind: "REJECTED_TO_SALES",
            procurementConfirmationId: business.procurement_confirmation_id,
            salesOrderId: business.sales_order_id,
            salesOrderNo: input.decision.salesOrderNo,
            rejectedSubmissionId: business.rejected_submission_id,
            rejectedSubjectHash: input.decision.subjectHash,
            workflowActionId: business.workflow_action_id,
            nextSalesResolutions: [
                "RESUBMIT_CHANGED_TERMS",
                "REQUEST_LOW_MARGIN_ACCEPTANCE",
                "VOID_AFTER_REJECTION",
            ],
            reference: business.workflow_action_id,
            rejectReasonCode: input.decision.rejectReasonCode,
            comment: input.decision.comment,
        }
        return { status: "succeeded", outcome }
    } catch (error) {
        if (
            isApiError(error) &&
            (error.kind === "Network" || error.kind === "Parse")
        ) {
            return {
                status: "unknown",
                idempotencyKey: input.idempotencyKey,
                message:
                    "请求结果尚未确认；请按操作号查询处理结果，确认前不得再次提交或打开下一项",
            }
        }
        return {
            status: "failed",
            message: apiErrorMessage(error),
            code: apiErrorCode(error),
        }
    }
}

function toConfirmationLinePayload(lines: ConfirmationLineDraft[]) {
    return lines.map((line, index) => ({
        line_no: index + 1,
        sales_order_submission_line_id: line.submissionLineId,
        supplier_id: line.supplierId,
        supplier_offering_revision_id: line.offeringRevisionId,
        confirmed_quantity: line.confirmedQuantity,
        latest_cost_gross: line.latestCostGross,
        input_tax_rate: line.inputTaxRate,
        expected_delivery_date: line.expectedDeliveryDate,
        fulfillment_mode: line.fulfillmentMode,
        supplier_capability_revision_id: line.capabilityRevisionId,
    }))
}
