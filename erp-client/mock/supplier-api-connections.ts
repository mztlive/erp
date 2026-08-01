/**
 * W20 mock seed — API 供应商连接
 * 密钥正文永不出现；仅安全引用元数据。
 */

import type {
  CapabilityCode,
  CapabilityView,
  ConnectionCenterView,
  ConnectionListItem,
  HealthRecordView,
} from "@/features/supplier-api-connections/types"
import {
  CAPABILITY_LABEL,
  CATALOG_LABEL,
  ENVIRONMENT_LABEL,
  HEALTH_LABEL,
  HEALTH_TONE,
  STATUS_LABEL,
  STATUS_TONE,
} from "@/features/supplier-api-connections/types"

const PRODUCT_LEVEL_NOTE =
  "连接级能力声明 ≠ 每个商品可用；商品/供给/发布级能力见 W21 / W22"

function cap(
  code: CapabilityCode,
  status: "ENABLED" | "DISABLED",
  verification: CapabilityView["verification"],
  opts?: Partial<CapabilityView>
): CapabilityView {
  const verificationLabel =
    verification === "SUCCESS"
      ? "验证成功"
      : verification === "FAILED"
        ? "验证失败"
        : verification === "STALE"
          ? "验证陈旧"
          : "未验证"
  return {
    capabilityCode: code,
    capabilityLabel: CAPABILITY_LABEL[code],
    status,
    statusLabel: status === "ENABLED" ? "启用" : "停用",
    verification,
    verificationLabel,
    businessRequirement: opts?.businessRequirement ?? "UNCONFIRMED",
    businessRequirementLabel:
      opts?.businessRequirement === "REQUIRED"
        ? "采购确认需要"
        : opts?.businessRequirement === "NOT_REQUIRED"
          ? "采购确认不需要"
          : "业务需求未确认",
    version: opts?.version ?? "cv1",
    productLevelNote: PRODUCT_LEVEL_NOTE,
    constraintSummary: opts?.constraintSummary,
    verifiedAt: opts?.verifiedAt,
  }
}

export type SeedConnection = {
  connectionId: string
  connectionCode: string
  supplier: { id: string; name: string }
  environment: ConnectionCenterView["environment"]
  status: ConnectionCenterView["status"]
  businessOwner?: { id: string; label: string }
  technicalOwner?: { id: string; label: string }
  adapter?: { code: string; version: string }
  version: string
  updatedAt: string
  endpoint: {
    state: "MISSING" | "BOUND" | "ROTATION_DUE"
    alias?: string
    version?: string
  }
  credential: {
    state: "MISSING" | "BOUND" | "ROTATION_DUE"
    alias?: string
    version?: string
  }
  capabilities: CapabilityView[]
  lastHealth?: ConnectionCenterView["lastHealth"]
  healthRecords: HealthRecordView[]
  catalog: ConnectionCenterView["catalog"]
  relatedImpact: ConnectionCenterView["relatedImpact"]
  auditEvents: ConnectionCenterView["auditEvents"]
  nextStep: string
  alerts?: ConnectionCenterView["alerts"]
}

export const CREDENTIAL_OPAQUE_OPTIONS = [
  {
    referenceId: "kms_ref_jd_prod_v3",
    alias: "kms://jd-enterprise/prod-primary",
    version: "v3",
  },
  {
    referenceId: "kms_ref_jd_prod_v4",
    alias: "kms://jd-enterprise/prod-rotate-candidate",
    version: "v4",
  },
  {
    referenceId: "kms_ref_sf_stg_v1",
    alias: "kms://sf-city/stg",
    version: "v1",
  },
  {
    referenceId: "kms_ref_meituan_prod_v2",
    alias: "kms://meituan-welfare/prod",
    version: "v2",
  },
] as const

export const SEED_CONNECTIONS: SeedConnection[] = [
  {
    connectionId: "conn_jd_prod",
    connectionCode: "CONN-JD-PROD",
    supplier: { id: "sup_jd", name: "京东企业购" },
    environment: "PRODUCTION",
    status: "ENABLED",
    businessOwner: { id: "u_zhao", label: "赵强" },
    technicalOwner: { id: "u_ops", label: "运维组" },
    adapter: { code: "jd-enterprise", version: "2.4.1" },
    version: "12",
    updatedAt: "2026-08-01T09:00:00+08:00",
    endpoint: {
      state: "BOUND",
      alias: "cfg://jd-prod-endpoint",
      version: "ep-v2",
    },
    credential: {
      state: "BOUND",
      alias: "kms://jd-enterprise/prod-primary",
      version: "v3",
    },
    capabilities: [
      cap("CATALOG", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
        verifiedAt: "2026-08-01T09:00:00+08:00",
        constraintSummary: "日同步窗口 02:00–06:00",
      }),
      cap("PRICE", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
        verifiedAt: "2026-08-01T09:00:00+08:00",
      }),
      cap("STOCK", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
        verifiedAt: "2026-08-01T09:00:00+08:00",
      }),
      cap("ORDER", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
        verifiedAt: "2026-08-01T09:00:00+08:00",
      }),
      cap("QUERY", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
        verifiedAt: "2026-08-01T09:00:00+08:00",
      }),
      cap("SETTLEMENT", "DISABLED", "UNVERIFIED", {
        businessRequirement: "NOT_REQUIRED",
      }),
    ],
    lastHealth: {
      at: "2026-08-01T09:00:00+08:00",
      result: "SUCCESS",
      resultLabel: HEALTH_LABEL.SUCCESS,
      latencyMs: 186,
      traceId: "tr_jd_h_0900",
    },
    healthRecords: [
      {
        recordId: "hr_jd_1",
        at: "2026-08-01T09:00:00+08:00",
        checkType: "全能力健康检查",
        result: "SUCCESS",
        resultLabel: HEALTH_LABEL.SUCCESS,
        resultTone: HEALTH_TONE.SUCCESS,
        latencyMs: 186,
        traceId: "tr_jd_h_0900",
        jobId: "job_h_jd_1",
        jobNo: "HLTH-JD-0900",
      },
      {
        recordId: "hr_jd_0",
        at: "2026-07-31T21:00:00+08:00",
        checkType: "鉴权探测",
        result: "SUCCESS",
        resultLabel: HEALTH_LABEL.SUCCESS,
        resultTone: HEALTH_TONE.SUCCESS,
        latencyMs: 120,
        traceId: "tr_jd_h_2100",
        jobId: "job_h_jd_0",
        jobNo: "HLTH-JD-2100",
      },
    ],
    catalog: {
      state: "FRESH",
      stateLabel: CATALOG_LABEL.FRESH,
      lastSuccessfulAt: "2026-08-01T03:12:00+08:00",
    },
    relatedImpact: {
      activeOfferings: 428,
      activePublications: 312,
      openSupplierOrders: 6,
      activeSyncJobs: 0,
    },
    auditEvents: [
      {
        eventId: "ae_jd_1",
        at: "2026-07-28T14:20:00+08:00",
        actor: "运维组",
        action: "BIND_CREDENTIAL_REFERENCE",
        summary: "密钥引用轮换至 v3（仅别名）",
        auditNo: "AUD-W20-7821",
      },
      {
        eventId: "ae_jd_2",
        at: "2026-07-20T10:00:00+08:00",
        actor: "赵强",
        action: "CONFIRM_CAPABILITY_REQUIREMENT",
        summary: "确认 CATALOG/PRICE/STOCK/ORDER 业务需要",
        auditNo: "AUD-W20-7602",
      },
    ],
    nextStep: "运行正常 · 可进入 W21 查看目录",
  },
  {
    connectionId: "conn_sf_stg",
    connectionCode: "CONN-SF-STG",
    supplier: { id: "sup_sf", name: "顺丰同城" },
    environment: "STAGING",
    status: "FAULTED",
    businessOwner: { id: "u_chen", label: "陈璐" },
    technicalOwner: { id: "u_plat", label: "平台组" },
    adapter: { code: "sf-city", version: "1.9.0" },
    version: "7",
    updatedAt: "2026-08-01T08:40:00+08:00",
    endpoint: {
      state: "BOUND",
      alias: "cfg://sf-stg-endpoint",
      version: "ep-v1",
    },
    credential: {
      state: "ROTATION_DUE",
      alias: "kms://sf-city/stg",
      version: "v1",
    },
    capabilities: [
      cap("CATALOG", "ENABLED", "FAILED", {
        businessRequirement: "REQUIRED",
        verifiedAt: "2026-08-01T08:40:00+08:00",
      }),
      cap("ORDER", "ENABLED", "FAILED", {
        businessRequirement: "REQUIRED",
      }),
      cap("LOGISTICS", "ENABLED", "STALE", {
        businessRequirement: "REQUIRED",
      }),
      cap("CALLBACK", "ENABLED", "UNVERIFIED"),
    ],
    lastHealth: {
      at: "2026-08-01T08:40:00+08:00",
      result: "AUTH_FAILED",
      resultLabel: HEALTH_LABEL.AUTH_FAILED,
      latencyMs: 42,
      traceId: "tr_sf_auth_fail",
      autoRetryStopped: true,
      errorClass: "AUTH_SIGNATURE_FAILURE",
      errorSummary:
        "鉴权/签名失败。自动重试已停止。请运维检查密钥引用与适配器，不得在本页输入明文密钥。",
    },
    healthRecords: [
      {
        recordId: "hr_sf_1",
        at: "2026-08-01T08:40:00+08:00",
        checkType: "鉴权探测",
        result: "AUTH_FAILED",
        resultLabel: HEALTH_LABEL.AUTH_FAILED,
        resultTone: HEALTH_TONE.AUTH_FAILED,
        latencyMs: 42,
        errorClass: "AUTH_SIGNATURE_FAILURE",
        errorSummary: "签名校验失败 · 自动重试已停止",
        autoRetryStopped: true,
        traceId: "tr_sf_auth_fail",
        jobId: "job_h_sf_1",
        jobNo: "HLTH-SF-0840",
      },
    ],
    catalog: {
      state: "STALE",
      stateLabel: CATALOG_LABEL.STALE,
      lastSuccessfulAt: "2026-07-28T04:00:00+08:00",
    },
    relatedImpact: {
      activeOfferings: 12,
      activePublications: 0,
      openSupplierOrders: 0,
      activeSyncJobs: 0,
    },
    auditEvents: [
      {
        eventId: "ae_sf_1",
        at: "2026-08-01T08:41:00+08:00",
        actor: "系统",
        action: "HEALTH_AUTH_FAILED",
        summary: "鉴权失败，自动重试停止",
        auditNo: "AUD-W20-8001",
      },
    ],
    nextStep: "轮换密钥引用并重新健康检查",
    alerts: [
      {
        id: "al_sf_auth",
        severity: "destructive",
        title: "鉴权/签名失败 · 自动重试已停止",
        description:
          "不得把鉴权失败当作临时网络抖动自动重试。请运维使用密钥管理系统不透明引用轮换后验证；本页无明文密钥输入。",
      },
    ],
  },
  {
    connectionId: "conn_mt_prod",
    connectionCode: "CONN-MT-PROD",
    supplier: { id: "sup_mt", name: "美团企业版" },
    environment: "PRODUCTION",
    status: "PENDING_CONFIG",
    businessOwner: { id: "u_zhao", label: "赵强" },
    technicalOwner: { id: "u_ops", label: "运维组" },
    adapter: { code: "meituan-welfare", version: "0.8.2" },
    version: "2",
    updatedAt: "2026-07-30T16:00:00+08:00",
    endpoint: { state: "MISSING" },
    credential: { state: "MISSING" },
    capabilities: [
      cap("CATALOG", "DISABLED", "UNVERIFIED", {
        businessRequirement: "REQUIRED",
      }),
      cap("ORDER", "DISABLED", "UNVERIFIED", {
        businessRequirement: "REQUIRED",
      }),
      cap("PRICE", "DISABLED", "UNVERIFIED"),
    ],
    healthRecords: [],
    catalog: {
      state: "NEVER",
      stateLabel: CATALOG_LABEL.NEVER,
    },
    relatedImpact: {
      activeOfferings: 0,
      activePublications: 0,
      openSupplierOrders: 0,
      activeSyncJobs: 0,
    },
    auditEvents: [
      {
        eventId: "ae_mt_1",
        at: "2026-07-30T16:00:00+08:00",
        actor: "系统管理员",
        action: "CREATE_CONNECTION",
        summary: "创建连接身份 CONN-MT-PROD",
        auditNo: "AUD-W20-7550",
      },
    ],
    nextStep: "绑定地址/密钥引用 → 配置能力 → 健康检查",
    alerts: [
      {
        id: "al_mt_cfg",
        severity: "warning",
        title: "生产环境配置不完整",
        description:
          "地址与密钥引用均未绑定。待配置状态不是故障；完成技术绑定与能力验证后方可启用。",
      },
    ],
  },
  {
    connectionId: "conn_kl_prod",
    connectionCode: "CONN-KL-PROD",
    supplier: { id: "sup_kl", name: "考拉企业购" },
    environment: "PRODUCTION",
    status: "ENABLED",
    businessOwner: { id: "u_li", label: "李倩" },
    technicalOwner: { id: "u_plat", label: "平台组" },
    adapter: { code: "kaola-enterprise", version: "1.3.0" },
    version: "4",
    updatedAt: "2026-08-01T07:45:00+08:00",
    endpoint: {
      state: "BOUND",
      alias: "cfg://kaola-prod-endpoint",
      version: "ep-v1",
    },
    credential: {
      state: "BOUND",
      alias: "kms://kaola/prod",
      version: "v2",
    },
    capabilities: [
      cap("CATALOG", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
        verifiedAt: "2026-08-01T07:45:00+08:00",
      }),
      cap("PRICE", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
        verifiedAt: "2026-08-01T07:45:00+08:00",
      }),
      cap("ORDER", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
        verifiedAt: "2026-08-01T07:45:00+08:00",
      }),
    ],
    lastHealth: {
      at: "2026-08-01T07:45:00+08:00",
      result: "SUCCESS",
      resultLabel: HEALTH_LABEL.SUCCESS,
      latencyMs: 260,
    },
    healthRecords: [],
    catalog: {
      state: "FRESH",
      stateLabel: CATALOG_LABEL.FRESH,
      lastSuccessfulAt: "2026-08-01T03:20:00+08:00",
    },
    relatedImpact: {
      activeOfferings: 88,
      activePublications: 52,
      openSupplierOrders: 2,
      activeSyncJobs: 0,
    },
    auditEvents: [],
    nextStep: "运行正常 · 可进入 W21 查看目录",
  },
  {
    connectionId: "conn_elm_prod",
    connectionCode: "CONN-ELM-PROD",
    supplier: { id: "sup_elm", name: "饿了么企业订餐" },
    environment: "PRODUCTION",
    status: "ENABLED",
    businessOwner: { id: "u_li", label: "李倩" },
    technicalOwner: { id: "u_ops", label: "运维组" },
    adapter: { code: "eleme-corp", version: "3.1.0" },
    version: "9",
    updatedAt: "2026-08-01T07:30:00+08:00",
    endpoint: {
      state: "BOUND",
      alias: "cfg://elm-prod-endpoint",
      version: "ep-v3",
    },
    credential: {
      state: "BOUND",
      alias: "kms://eleme-corp/prod",
      version: "v5",
    },
    capabilities: [
      cap("CATALOG", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
        verifiedAt: "2026-08-01T07:30:00+08:00",
      }),
      cap("ORDER", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
        verifiedAt: "2026-08-01T07:30:00+08:00",
      }),
      cap("CANCEL", "ENABLED", "FAILED", {
        businessRequirement: "REQUIRED",
      }),
      cap("LOGISTICS", "ENABLED", "STALE", {
        businessRequirement: "REQUIRED",
        verifiedAt: "2026-07-20T10:00:00+08:00",
      }),
    ],
    lastHealth: {
      at: "2026-08-01T07:30:00+08:00",
      result: "PARTIAL",
      resultLabel: HEALTH_LABEL.PARTIAL,
      latencyMs: 310,
      traceId: "tr_elm_partial",
    },
    healthRecords: [
      {
        recordId: "hr_elm_1",
        at: "2026-08-01T07:30:00+08:00",
        checkType: "全能力健康检查",
        result: "PARTIAL",
        resultLabel: HEALTH_LABEL.PARTIAL,
        resultTone: HEALTH_TONE.PARTIAL,
        latencyMs: 310,
        errorSummary: "物流能力验证陈旧，其余通过",
        traceId: "tr_elm_partial",
        jobId: "job_h_elm_1",
        jobNo: "HLTH-ELM-0730",
      },
    ],
    catalog: {
      state: "STALE",
      stateLabel: CATALOG_LABEL.STALE,
      lastSuccessfulAt: "2026-07-25T02:00:00+08:00",
    },
    relatedImpact: {
      activeOfferings: 86,
      activePublications: 54,
      openSupplierOrders: 3,
      activeSyncJobs: 0,
    },
    auditEvents: [],
    nextStep: "触发目录同步并复查物流能力",
    alerts: [
      {
        id: "al_elm_cat",
        severity: "warning",
        title: "商品目录水位陈旧",
        description:
          "连接状态与目录水位分开展示。可触发目录同步（后台任务）或进入 W21 查看。",
      },
    ],
  },
  {
    connectionId: "conn_dd_prod",
    connectionCode: "CONN-DD-PROD",
    supplier: { id: "sup_dd", name: "叮咚买菜企业" },
    environment: "PRODUCTION",
    status: "DISABLED",
    businessOwner: { id: "u_zhao", label: "赵强" },
    technicalOwner: { id: "u_ops", label: "运维组" },
    adapter: { code: "dingdong-corp", version: "1.2.0" },
    version: "5",
    updatedAt: "2026-07-15T11:00:00+08:00",
    endpoint: {
      state: "BOUND",
      alias: "cfg://dd-prod-endpoint",
      version: "ep-v1",
    },
    credential: {
      state: "BOUND",
      alias: "kms://dingdong/prod",
      version: "v2",
    },
    capabilities: [
      cap("CATALOG", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
      }),
      cap("ORDER", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
      }),
    ],
    lastHealth: {
      at: "2026-07-15T10:50:00+08:00",
      result: "SUCCESS",
      resultLabel: HEALTH_LABEL.SUCCESS,
      latencyMs: 200,
    },
    healthRecords: [],
    catalog: {
      state: "FRESH",
      stateLabel: CATALOG_LABEL.FRESH,
      lastSuccessfulAt: "2026-07-15T03:00:00+08:00",
    },
    relatedImpact: {
      activeOfferings: 0,
      activePublications: 0,
      openSupplierOrders: 0,
      activeSyncJobs: 0,
    },
    auditEvents: [
      {
        eventId: "ae_dd_1",
        at: "2026-07-15T11:00:00+08:00",
        actor: "系统管理员",
        action: "DISABLE",
        summary: "停用连接；历史版本与业务记录保留",
        auditNo: "AUD-W20-7100",
      },
    ],
    nextStep: "已停用 · 历史保留 · 可重新启用（需验证）",
  },
  {
    connectionId: "conn_sn_prod",
    connectionCode: "CONN-SN-PROD",
    supplier: { id: "sup_sn", name: "苏宁易购企业" },
    environment: "PRODUCTION",
    status: "ENABLED",
    businessOwner: { id: "u_li", label: "李倩" },
    technicalOwner: { id: "u_plat", label: "平台组" },
    adapter: { code: "suning-b2b", version: "2.0.3" },
    version: "4",
    updatedAt: "2026-08-01T06:00:00+08:00",
    endpoint: {
      state: "BOUND",
      alias: "cfg://sn-prod-endpoint",
      version: "ep-v1",
    },
    credential: {
      state: "ROTATION_DUE",
      alias: "kms://suning/prod",
      version: "v1",
    },
    capabilities: [
      cap("CATALOG", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
        verifiedAt: "2026-08-01T06:00:00+08:00",
      }),
      cap("PRICE", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
      }),
      cap("ORDER", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
      }),
      cap("REFUND", "ENABLED", "UNVERIFIED"),
    ],
    lastHealth: {
      at: "2026-08-01T06:00:00+08:00",
      result: "SUCCESS",
      resultLabel: HEALTH_LABEL.SUCCESS,
      latencyMs: 240,
    },
    healthRecords: [
      {
        recordId: "hr_sn_1",
        at: "2026-08-01T06:00:00+08:00",
        checkType: "全能力健康检查",
        result: "SUCCESS",
        resultLabel: HEALTH_LABEL.SUCCESS,
        resultTone: HEALTH_TONE.SUCCESS,
        latencyMs: 240,
        jobId: "job_h_sn_1",
        jobNo: "HLTH-SN-0600",
      },
    ],
    catalog: {
      state: "RUNNING",
      stateLabel: CATALOG_LABEL.RUNNING,
      lastSuccessfulAt: "2026-07-31T03:00:00+08:00",
      activeJobId: "job_cat_sn_1",
      activeJobNo: "CAT-SN-0801",
      progress: {
        status: "running",
        total: 1200,
        completed: 480,
        succeeded: 470,
        failed: 10,
      },
    },
    relatedImpact: {
      activeOfferings: 210,
      activePublications: 180,
      openSupplierOrders: 2,
      activeSyncJobs: 1,
    },
    auditEvents: [],
    nextStep: "目录同步进行中 · 密钥引用建议轮换",
  },
  {
    connectionId: "conn_yx_dev",
    connectionCode: "CONN-YX-DEV",
    supplier: { id: "sup_yx", name: "网易严选企业" },
    environment: "DEVELOPMENT",
    status: "ENABLED",
    businessOwner: { id: "u_chen", label: "陈璐" },
    technicalOwner: { id: "u_plat", label: "平台组" },
    adapter: { code: "yanxuan-b2b", version: "0.3.0-dev" },
    version: "1",
    updatedAt: "2026-07-29T12:00:00+08:00",
    endpoint: {
      state: "BOUND",
      alias: "cfg://yx-dev-endpoint",
      version: "ep-dev",
    },
    credential: {
      state: "BOUND",
      alias: "kms://yanxuan/dev",
      version: "v0",
    },
    capabilities: [
      cap("CATALOG", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
      }),
      cap("ORDER", "DISABLED", "UNVERIFIED"),
    ],
    lastHealth: {
      at: "2026-07-29T12:00:00+08:00",
      result: "SUCCESS",
      resultLabel: HEALTH_LABEL.SUCCESS,
      latencyMs: 90,
    },
    healthRecords: [],
    catalog: {
      state: "FRESH",
      stateLabel: CATALOG_LABEL.FRESH,
      lastSuccessfulAt: "2026-07-29T11:50:00+08:00",
    },
    relatedImpact: {
      activeOfferings: 3,
      activePublications: 0,
      openSupplierOrders: 0,
      activeSyncJobs: 0,
    },
    auditEvents: [],
    nextStep: "开发环境 · 禁止对生产业务动作",
  },
  {
    connectionId: "conn_hw_prod",
    connectionCode: "CONN-HW-PROD",
    supplier: { id: "sup_hw", name: "华为商城企业" },
    environment: "PRODUCTION",
    status: "FAULTED",
    businessOwner: { id: "u_zhao", label: "赵强" },
    technicalOwner: { id: "u_ops", label: "运维组" },
    adapter: { code: "huawei-mall", version: "1.5.2" },
    version: "8",
    updatedAt: "2026-08-01T08:10:00+08:00",
    endpoint: {
      state: "BOUND",
      alias: "cfg://hw-prod-endpoint",
      version: "ep-v2",
    },
    credential: {
      state: "BOUND",
      alias: "kms://huawei/prod",
      version: "v4",
    },
    capabilities: [
      cap("CATALOG", "ENABLED", "FAILED", {
        businessRequirement: "REQUIRED",
      }),
      cap("ORDER", "ENABLED", "FAILED", {
        businessRequirement: "REQUIRED",
      }),
      cap("QUERY", "ENABLED", "UNVERIFIED", {
        businessRequirement: "REQUIRED",
      }),
    ],
    lastHealth: {
      at: "2026-08-01T08:10:00+08:00",
      result: "UNKNOWN",
      resultLabel: HEALTH_LABEL.UNKNOWN,
      traceId: "tr_hw_unknown",
      errorClass: "RESULT_UNKNOWN",
      errorSummary:
        "健康检查结果未知。不得按失败或成功处理；请按任务号查询最终结论，不盲目重复发起。",
    },
    healthRecords: [
      {
        recordId: "hr_hw_1",
        at: "2026-08-01T08:10:00+08:00",
        checkType: "全能力健康检查",
        result: "UNKNOWN",
        resultLabel: HEALTH_LABEL.UNKNOWN,
        resultTone: HEALTH_TONE.UNKNOWN,
        errorClass: "RESULT_UNKNOWN",
        errorSummary: "结果未知 · 按任务号查询",
        traceId: "tr_hw_unknown",
        jobId: "job_h_hw_1",
        jobNo: "HLTH-HW-0810",
      },
    ],
    catalog: {
      state: "FAILED",
      stateLabel: CATALOG_LABEL.FAILED,
      lastSuccessfulAt: "2026-07-20T03:00:00+08:00",
    },
    relatedImpact: {
      activeOfferings: 64,
      activePublications: 40,
      openSupplierOrders: 5,
      activeSyncJobs: 0,
    },
    auditEvents: [],
    nextStep: "按任务号查询健康结果 · 禁止盲目重试",
    alerts: [
      {
        id: "al_hw_unknown",
        severity: "warning",
        title: "健康检查结果未知",
        description:
          "处理结果不确定时不乐观改变启停或引用状态。请用原任务号查询最终结论。",
      },
    ],
  },
  {
    connectionId: "conn_wph_stg",
    connectionCode: "CONN-WPH-STG",
    supplier: { id: "sup_wph", name: "唯品会企业" },
    environment: "STAGING",
    status: "ENABLED",
    businessOwner: { id: "u_chen", label: "陈璐" },
    technicalOwner: { id: "u_plat", label: "平台组" },
    adapter: { code: "vip-corp", version: "1.1.0" },
    version: "3",
    updatedAt: "2026-07-31T18:00:00+08:00",
    endpoint: {
      state: "BOUND",
      alias: "cfg://wph-stg-endpoint",
      version: "ep-v1",
    },
    credential: {
      state: "BOUND",
      alias: "kms://vip/stg",
      version: "v1",
    },
    capabilities: [
      cap("CATALOG", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
      }),
      cap("PRICE", "ENABLED", "SUCCESS", {
        businessRequirement: "REQUIRED",
      }),
      cap("STOCK", "ENABLED", "SUCCESS"),
    ],
    lastHealth: {
      at: "2026-07-31T18:00:00+08:00",
      result: "STALE",
      resultLabel: HEALTH_LABEL.STALE,
      latencyMs: 150,
    },
    healthRecords: [],
    catalog: {
      state: "FRESH",
      stateLabel: CATALOG_LABEL.FRESH,
      lastSuccessfulAt: "2026-07-31T04:00:00+08:00",
    },
    relatedImpact: {
      activeOfferings: 8,
      activePublications: 0,
      openSupplierOrders: 0,
      activeSyncJobs: 0,
    },
    auditEvents: [],
    nextStep: "健康检查已陈旧 · 建议复检",
  },
]

function capabilitySummary(caps: CapabilityView[]): string {
  const enabled = caps.filter((c) => c.status === "ENABLED")
  if (enabled.length === 0) return "无启用能力"
  const labels = enabled.slice(0, 3).map((c) => c.capabilityLabel)
  const more = enabled.length > 3 ? `+${enabled.length - 3}` : ""
  return `${labels.join("、")}${more}`
}

export function seedToListItem(seed: SeedConnection): ConnectionListItem {
  const healthResult = seed.lastHealth?.result ?? "UNCHECKED"
  return {
    connectionId: seed.connectionId,
    connectionCode: seed.connectionCode,
    supplier: seed.supplier,
    environment: seed.environment,
    environmentLabel: ENVIRONMENT_LABEL[seed.environment],
    status: seed.status,
    statusLabel: STATUS_LABEL[seed.status],
    statusTone: STATUS_TONE[seed.status],
    capabilitySummary: capabilitySummary(seed.capabilities),
    healthResult,
    healthLabel: HEALTH_LABEL[healthResult],
    healthTone: HEALTH_TONE[healthResult],
    lastHealthAt: seed.lastHealth?.at,
    catalogState: seed.catalog.state,
    catalogLabel: seed.catalog.stateLabel,
    nextStep: seed.nextStep,
    businessOwner: seed.businessOwner?.label,
    technicalOwner: seed.technicalOwner?.label,
    allowedActions: [],
    actionBlockers: [],
  }
}
