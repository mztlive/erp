/**
 * W10 库存台账 · 展示辅助入口。
 * 纯逻辑见 lib/presentation；UI 辅助见 components/presentation；本文件只做再导出。
 */

export {
    adjustSchema,
    defaultSortValue,
    localNowInput,
    MOVEMENT_TYPE_OPTIONS,
    parseAvailability,
    parseView,
    sortOptions,
} from "./lib/presentation"
export { ChipFilter, formatQty } from "./components/presentation"
