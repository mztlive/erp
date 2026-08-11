export const positiveDecimal = (value: string | undefined) =>
    value === undefined ||
    value === "" ||
    (/^\d+(?:\.\d+)?$/.test(value) && Number(value) > 0)

export const taxRateValid = (value: string) =>
    value === "" ||
    (/^\d+(?:\.\d+)?$/.test(value) && Number(value) > 0 && Number(value) < 1)
