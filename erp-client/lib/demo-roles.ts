/**
 * demoRole / role URL 参数的通用解析与角色下拉选项构造。
 *
 * 各 feature 保留自己的角色枚举与中文名表，这里只收拢重复的
 * 「URL 参数校验 + 回默认」逻辑；非法或缺失一律返回 undefined，
 * 由调用方决定默认角色（即非 demo 模式）。
 */

/** 从 URL 参数解析演示角色；非法或缺失返回 undefined（非 demo 模式）。 */
export function parseDemoRole<T extends string>(
  param: string | null,
  roles: readonly T[]
): T | undefined {
  if (param && roles.includes(param as T)) return param as T
  return undefined
}

/** 由「角色 → 中文名」映射构造下拉选项（value = 角色 key）。 */
export function createRoleOptions(
  labels: Readonly<Record<string, string>>
): { value: string; label: string }[] {
  return Object.keys(labels).map((value) => ({ value, label: labels[value] }))
}
