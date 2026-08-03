import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  CreateCustomerInput,
  CustomerCenterView,
  CustomerDirectoryQuery,
  CustomerDirectoryResult,
  CustomerMutationResult,
  SaveCustomerDetailsInput,
  SaveCustomerRevisionInput,
} from "@/features/customers/types"
import { filterCustomerDirectory } from "@/features/customers/filter-customers"
import {
  createW03Customer,
  getW03DetailBaseline,
  getW03NoCustomerScope,
  getW03SensitiveReveal,
  listW03DirectoryBaseline,
  queryW03Idempotency,
  saveW03CustomerDetails,
  saveW03CustomerRevision,
} from "@/features/customers/session"

export async function fetchCustomerDirectory(
  query: CustomerDirectoryQuery
): Promise<CustomerDirectoryResult> {
  await mockDelay(90)

  if (getW03NoCustomerScope()) {
    return {
      hasCustomerScope: false,
      items: [],
      totalInScope: 0,
      queriedAt: new Date().toISOString(),
    }
  }

  const all = listW03DirectoryBaseline()
  const inScope = all.filter((item) => item.scopeTags.includes(query.scope))
  const items = filterCustomerDirectory(all, query)

  return {
    hasCustomerScope: true,
    items,
    totalInScope: inScope.length,
    queriedAt: new Date().toISOString(),
  }
}

export async function fetchCustomerCenter(
  customerId: string
): Promise<CustomerCenterView | null> {
  await mockDelay(100)
  return getW03DetailBaseline(customerId)
}

export async function saveCustomerRevision(
  input: SaveCustomerRevisionInput
): Promise<CustomerMutationResult> {
  await mockDelay(120)
  return saveW03CustomerRevision(input)
}

export async function saveCustomerDetails(
  input: SaveCustomerDetailsInput
): Promise<CustomerMutationResult> {
  await mockDelay(140)
  return saveW03CustomerDetails(input)
}

export async function createCustomer(
  input: CreateCustomerInput
): Promise<CustomerMutationResult> {
  await mockDelay(140)
  return createW03Customer(input)
}

export async function queryCustomerMutationByIdempotency(
  idempotencyKey: string
): Promise<CustomerMutationResult | null> {
  await mockDelay(60)
  return queryW03Idempotency(idempotencyKey)
}

/** Short-lived reveal; plaintext never appears in directory/detail payloads. */
export async function revealCustomerSensitiveField(
  revealToken: string
): Promise<string> {
  await mockDelay(80)
  const value = getW03SensitiveReveal(revealToken)
  if (!value) {
    throw new Error("无权查看或权限已失效")
  }
  return value
}
