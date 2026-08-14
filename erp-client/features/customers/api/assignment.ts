import { apiPost } from "@/lib/api"
import type {
    CustomerAssignmentChangeInput,
    CustomerAssignmentView,
} from "@/features/customers/types"
import { mapAssignment } from "./mappers"
import type { BackendAssignment } from "./wire-types"

/** 建立、换任或结束客户责任归属。 */
export async function applyCustomerAssignment(
    input: CustomerAssignmentChangeInput,
): Promise<CustomerAssignmentView[]> {
    const rows = await apiPost<BackendAssignment[]>(
        `/admin/customers/${input.customerId}/assignments`,
        {
            action: input.action,
            user_id: input.userId,
            assignment_role: input.role,
            valid_from: input.effectiveFrom,
            valid_to: input.effectiveTo,
            assignment_id: input.assignmentId,
            change_reason: input.changeReason.trim(),
            version: input.version,
        },
    )
    return rows.map(mapAssignment)
}
