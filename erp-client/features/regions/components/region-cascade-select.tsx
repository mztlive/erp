"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business/option-combobox"
import { cn } from "@/lib/utils"

import { useRegionIndex } from "../hooks/queries"
import { isLeafRegion, regionPath, type RegionIndex } from "../lib/region-index"
import type { RegionNode } from "../api"

type RegionCascadeSelectProps = {
    /** 三级选择框的 ID 前缀，派生 `-province`、`-city`、`-district`。 */
    idPrefix: string
    /** 当前选中的区划代码；空字符串表示未选择。 */
    value: string
    onValueChange: (code: string) => void
    /** 收货地址须选到最末级：未选完前回传空字符串。 */
    leafOnly?: boolean
    disabled?: boolean
    invalid?: boolean
    onBlur?: () => void
    ariaLabel?: string
    ariaDescribedBy?: string
    className?: string
}

function toOptions(nodes: readonly RegionNode[] | undefined) {
    return (nodes ?? []).map((node) => ({ value: node.code, label: node.name }))
}

function levels(index: RegionIndex | undefined, value: string) {
    const path = index && value ? regionPath(index, value) : []
    return {
        province: path[0]?.code ?? "",
        city: path[1]?.code ?? "",
        district: path[2]?.code ?? "",
    }
}

/**
 * 省、市、区县三级选择。任意级模式下每级选择后立即回传当前最具体的代码；
 * 最末级模式下只在选到没有下级的区划时回传代码，未选完前回传空字符串。
 */
export function RegionCascadeSelect({
    idPrefix,
    value,
    onValueChange,
    leafOnly = false,
    disabled = false,
    invalid = false,
    onBlur,
    ariaLabel = "地区",
    ariaDescribedBy,
    className,
}: RegionCascadeSelectProps) {
    const { data: index, isFetching, refetch } = useRegionIndex()
    // 最末级模式下未选完的省、市只是界面草稿，不进入表单值。
    const [draft, setDraft] = React.useState({ province: "", city: "" })
    const fromValue = levels(index, value)
    const selected = value
        ? fromValue
        : { province: draft.province, city: draft.city, district: "" }
    const provinces = React.useMemo(() => toOptions(index?.provinces), [index])
    const cities = React.useMemo(
        () => toOptions(index?.children.get(selected.province)),
        [index, selected.province],
    )
    const districts = React.useMemo(
        () => toOptions(index?.children.get(selected.city)),
        [index, selected.city],
    )
    const emit = (code: string) => {
        if (!leafOnly || !code || (index && isLeafRegion(index, code))) {
            onValueChange(code)
        } else {
            onValueChange("")
        }
    }
    const chooseProvince = (code: string) => {
        setDraft({ province: code, city: "" })
        emit(code)
    }
    const chooseCity = (code: string) => {
        setDraft({ province: selected.province, city: code })
        emit(code || (leafOnly ? "" : selected.province))
    }
    const chooseDistrict = (code: string) => {
        setDraft({ province: selected.province, city: selected.city })
        emit(code || (leafOnly ? "" : selected.city))
    }
    const common = {
        disabled,
        loading: isFetching,
        onBlur,
        "aria-invalid": invalid || undefined,
        "aria-describedby": ariaDescribedBy,
        inputClassName: "w-full",
    }

    return (
        <div className={cn("grid min-w-0 grid-cols-3 gap-2", className)}>
            <OptionCombobox
                onRefresh={() => refetch({ throwOnError: true })}
                {...common}
                id={`${idPrefix}-province`}
                aria-label={`${ariaLabel}（省）`}
                options={provinces}
                value={selected.province || null}
                placeholder="省/直辖市"
                onValueChange={(code) => chooseProvince(code ?? "")}
            />
            <OptionCombobox
                onRefresh={() => refetch({ throwOnError: true })}
                {...common}
                id={`${idPrefix}-city`}
                aria-label={`${ariaLabel}（市）`}
                options={cities}
                value={selected.city || null}
                placeholder={cities.length ? "市" : "—"}
                disabled={disabled || !cities.length}
                onValueChange={(code) => chooseCity(code ?? "")}
            />
            <OptionCombobox
                onRefresh={() => refetch({ throwOnError: true })}
                {...common}
                id={`${idPrefix}-district`}
                aria-label={`${ariaLabel}（区县）`}
                options={districts}
                value={selected.district || null}
                placeholder={districts.length ? "区/县" : "—"}
                disabled={disabled || !districts.length}
                onValueChange={(code) => chooseDistrict(code ?? "")}
            />
        </div>
    )
}
