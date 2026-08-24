"use client"

import { useId } from "react"

import { cn } from "@/lib/utils"

import "./login-stage.css"

type LoginStageArtProps = {
    className?: string
}

/**
 * 登录品牌区装饰 SVG：网格、轨道、单据卡与沿路径流动的数据点。
 * 纯装饰，不参与交互。
 */
export function LoginStageArt({ className }: LoginStageArtProps) {
    const uid = useId().replace(/:/g, "")
    const gridId = `${uid}-grid`
    const fadeId = `${uid}-fade`
    const glowId = `${uid}-glow`
    const washId = `${uid}-wash`

    return (
        <svg
            className={cn("login-stage-art", className)}
            viewBox="0 0 960 1080"
            preserveAspectRatio="xMidYMid slice"
            fill="none"
            aria-hidden="true"
            focusable="false"
        >
            <defs>
                <pattern
                    id={gridId}
                    width="48"
                    height="48"
                    patternUnits="userSpaceOnUse"
                >
                    <path
                        d="M 48 0 L 0 0 0 48"
                        stroke="currentColor"
                        strokeWidth="0.7"
                        opacity="0.16"
                    />
                </pattern>
                <radialGradient id={washId} cx="50%" cy="42%" r="62%">
                    <stop
                        offset="0%"
                        stopColor="currentColor"
                        stopOpacity="0.16"
                    />
                    <stop
                        offset="55%"
                        stopColor="currentColor"
                        stopOpacity="0.04"
                    />
                    <stop
                        offset="100%"
                        stopColor="currentColor"
                        stopOpacity="0"
                    />
                </radialGradient>
                <linearGradient
                    id={fadeId}
                    x1="0"
                    y1="0"
                    x2="0"
                    y2="1"
                    gradientUnits="objectBoundingBox"
                >
                    <stop
                        offset="0%"
                        stopColor="currentColor"
                        stopOpacity="0"
                    />
                    <stop
                        offset="35%"
                        stopColor="currentColor"
                        stopOpacity="0.18"
                    />
                    <stop
                        offset="100%"
                        stopColor="currentColor"
                        stopOpacity="0"
                    />
                </linearGradient>
                <filter
                    id={glowId}
                    x="-40%"
                    y="-40%"
                    width="180%"
                    height="180%"
                >
                    <feGaussianBlur stdDeviation="10" result="blur" />
                    <feMerge>
                        <feMergeNode in="blur" />
                        <feMergeNode in="SourceGraphic" />
                    </feMerge>
                </filter>
            </defs>

            <rect width="960" height="1080" fill={`url(#${washId})`} />
            <g className="login-grid">
                <rect
                    x="-48"
                    y="-48"
                    width="1056"
                    height="1176"
                    fill={`url(#${gridId})`}
                />
            </g>
            <rect width="960" height="1080" fill={`url(#${fadeId})`} />

            <ellipse
                className="login-orb"
                cx="120"
                cy="220"
                rx="36"
                ry="36"
                fill="currentColor"
                opacity="0.06"
            />
            <ellipse
                className="login-orb login-orb-b"
                cx="900"
                cy="940"
                rx="48"
                ry="48"
                fill="currentColor"
                opacity="0.05"
            />

            <g className="login-cloud" opacity="0.55">
                <ellipse
                    cx="430"
                    cy="168"
                    rx="72"
                    ry="28"
                    fill="currentColor"
                    opacity="0.16"
                />
                <ellipse
                    cx="490"
                    cy="158"
                    rx="58"
                    ry="24"
                    fill="currentColor"
                    opacity="0.2"
                />
                <ellipse
                    cx="540"
                    cy="172"
                    rx="64"
                    ry="26"
                    fill="currentColor"
                    opacity="0.14"
                />
                <path
                    d="M392 186c12-28 46-44 84-40 18-22 54-30 86-16 28-8 62 4 74 28 28 4 48 24 46 48H386c-8-18 0-36 6-20z"
                    stroke="currentColor"
                    strokeWidth="1.2"
                    opacity="0.45"
                />
            </g>

            <circle
                className="login-ring login-ring-a"
                cx="480"
                cy="500"
                r="148"
                strokeWidth="1"
                strokeDasharray="4 14"
                opacity="0.35"
            />
            <circle
                className="login-ring login-ring-b"
                cx="480"
                cy="500"
                r="228"
                strokeWidth="1"
                strokeDasharray="2 18"
                opacity="0.28"
            />
            <circle
                className="login-ring login-ring-c"
                cx="480"
                cy="500"
                r="318"
                strokeWidth="0.8"
                strokeDasharray="1 22"
                opacity="0.2"
            />

            <path
                className="login-flow"
                d="M 480 500 C 360 430 280 400 210 360"
                strokeWidth="1.4"
                strokeDasharray="6 10"
                opacity="0.45"
            />
            <path
                className="login-flow"
                d="M 480 500 C 600 430 680 400 750 360"
                strokeWidth="1.4"
                strokeDasharray="6 10"
                opacity="0.45"
            />
            <path
                className="login-flow login-flow-slow"
                d="M 480 500 C 380 600 300 690 250 740"
                strokeWidth="1.4"
                strokeDasharray="5 12"
                opacity="0.4"
            />
            <path
                className="login-flow login-flow-slow"
                d="M 480 500 C 580 600 660 690 720 740"
                strokeWidth="1.4"
                strokeDasharray="5 12"
                opacity="0.4"
            />
            <path
                className="login-flow"
                d="M 210 360 C 320 250 640 250 750 360"
                strokeWidth="1"
                strokeDasharray="3 14"
                opacity="0.28"
            />

            <circle
                className="login-packet login-packet-a"
                r="3.5"
                fill="currentColor"
            />
            <circle
                className="login-packet login-packet-b"
                r="3.5"
                fill="currentColor"
            />
            <circle
                className="login-packet login-packet-c"
                r="3"
                fill="currentColor"
            />
            <circle
                className="login-packet login-packet-d"
                r="3"
                fill="currentColor"
            />
            <circle
                className="login-packet login-packet-e"
                r="2.5"
                fill="currentColor"
            />

            <g className="login-float-a" filter={`url(#${glowId})`}>
                <rect
                    x="372"
                    y="332"
                    width="216"
                    height="248"
                    rx="18"
                    fill="currentColor"
                    opacity="0.08"
                    stroke="currentColor"
                    strokeOpacity="0.42"
                    strokeWidth="1.2"
                />
                <rect
                    x="394"
                    y="358"
                    width="92"
                    height="10"
                    rx="5"
                    fill="currentColor"
                    opacity="0.55"
                />
                <rect
                    x="394"
                    y="378"
                    width="54"
                    height="6"
                    rx="3"
                    fill="currentColor"
                    opacity="0.28"
                />
                <rect
                    x="394"
                    y="414"
                    width="172"
                    height="6"
                    rx="3"
                    fill="currentColor"
                    opacity="0.22"
                />
                <rect
                    x="394"
                    y="436"
                    width="148"
                    height="6"
                    rx="3"
                    fill="currentColor"
                    opacity="0.18"
                />
                <rect
                    x="394"
                    y="458"
                    width="164"
                    height="6"
                    rx="3"
                    fill="currentColor"
                    opacity="0.16"
                />
                <rect
                    x="394"
                    y="480"
                    width="120"
                    height="6"
                    rx="3"
                    fill="currentColor"
                    opacity="0.14"
                />
                <rect
                    x="394"
                    y="520"
                    width="72"
                    height="28"
                    rx="8"
                    fill="currentColor"
                    opacity="0.2"
                />
                <rect
                    x="478"
                    y="520"
                    width="88"
                    height="28"
                    rx="8"
                    fill="currentColor"
                    opacity="0.12"
                    stroke="currentColor"
                    strokeOpacity="0.3"
                />
                <rect
                    className="login-scan"
                    x="378"
                    y="348"
                    width="204"
                    height="14"
                    fill="currentColor"
                    opacity="0.06"
                />
            </g>

            <g className="login-float-b">
                <rect
                    x="132"
                    y="308"
                    width="156"
                    height="104"
                    rx="16"
                    fill="currentColor"
                    opacity="0.08"
                    stroke="currentColor"
                    strokeOpacity="0.38"
                />
                <circle
                    className="login-pulse"
                    cx="164"
                    cy="344"
                    r="8"
                    fill="currentColor"
                    opacity="0.7"
                />
                <rect
                    x="182"
                    y="336"
                    width="78"
                    height="8"
                    rx="4"
                    fill="currentColor"
                    opacity="0.5"
                />
                <rect
                    x="182"
                    y="354"
                    width="52"
                    height="6"
                    rx="3"
                    fill="currentColor"
                    opacity="0.24"
                />
                <rect
                    x="152"
                    y="378"
                    width="116"
                    height="6"
                    rx="3"
                    fill="currentColor"
                    opacity="0.16"
                />
                <text
                    x="182"
                    y="398"
                    fill="currentColor"
                    opacity="0.45"
                    fontSize="11"
                    fontFamily="ui-sans-serif, system-ui, sans-serif"
                >
                    销售
                </text>
            </g>

            <g className="login-float-c">
                <rect
                    x="672"
                    y="308"
                    width="156"
                    height="104"
                    rx="16"
                    fill="currentColor"
                    opacity="0.08"
                    stroke="currentColor"
                    strokeOpacity="0.38"
                />
                <circle
                    className="login-pulse login-pulse-delay"
                    cx="704"
                    cy="344"
                    r="8"
                    fill="currentColor"
                    opacity="0.7"
                />
                <rect
                    x="722"
                    y="336"
                    width="78"
                    height="8"
                    rx="4"
                    fill="currentColor"
                    opacity="0.5"
                />
                <rect
                    x="722"
                    y="354"
                    width="52"
                    height="6"
                    rx="3"
                    fill="currentColor"
                    opacity="0.24"
                />
                <rect
                    x="692"
                    y="378"
                    width="116"
                    height="6"
                    rx="3"
                    fill="currentColor"
                    opacity="0.16"
                />
                <text
                    x="722"
                    y="398"
                    fill="currentColor"
                    opacity="0.45"
                    fontSize="11"
                    fontFamily="ui-sans-serif, system-ui, sans-serif"
                >
                    采购
                </text>
            </g>

            <g className="login-float-d">
                <rect
                    x="168"
                    y="688"
                    width="164"
                    height="104"
                    rx="16"
                    fill="currentColor"
                    opacity="0.08"
                    stroke="currentColor"
                    strokeOpacity="0.38"
                />
                <rect
                    x="188"
                    y="712"
                    width="64"
                    height="8"
                    rx="4"
                    fill="currentColor"
                    opacity="0.45"
                />
                <rect
                    x="188"
                    y="736"
                    width="124"
                    height="6"
                    rx="3"
                    fill="currentColor"
                    opacity="0.18"
                />
                <rect
                    x="188"
                    y="754"
                    width="96"
                    height="6"
                    rx="3"
                    fill="currentColor"
                    opacity="0.14"
                />
                <text
                    x="188"
                    y="776"
                    fill="currentColor"
                    opacity="0.45"
                    fontSize="11"
                    fontFamily="ui-sans-serif, system-ui, sans-serif"
                >
                    库存
                </text>
            </g>

            <g className="login-float-e">
                <rect
                    x="628"
                    y="688"
                    width="164"
                    height="104"
                    rx="16"
                    fill="currentColor"
                    opacity="0.08"
                    stroke="currentColor"
                    strokeOpacity="0.38"
                />
                <rect
                    x="648"
                    y="712"
                    width="64"
                    height="8"
                    rx="4"
                    fill="currentColor"
                    opacity="0.45"
                />
                <rect
                    x="648"
                    y="736"
                    width="124"
                    height="6"
                    rx="3"
                    fill="currentColor"
                    opacity="0.18"
                />
                <rect
                    x="648"
                    y="754"
                    width="96"
                    height="6"
                    rx="3"
                    fill="currentColor"
                    opacity="0.14"
                />
                <text
                    x="648"
                    y="776"
                    fill="currentColor"
                    opacity="0.45"
                    fontSize="11"
                    fontFamily="ui-sans-serif, system-ui, sans-serif"
                >
                    结算
                </text>
            </g>
        </svg>
    )
}

type LoginMarkProps = {
    className?: string
    inverted?: boolean
}

/** 登录页品牌徽标：云形弧线叠账本横线。 */
export function LoginMark({ className, inverted = false }: LoginMarkProps) {
    return (
        <svg
            viewBox="0 0 40 40"
            className={cn("size-10", className)}
            aria-hidden="true"
            focusable="false"
        >
            <rect
                width="40"
                height="40"
                rx="11"
                className={inverted ? "fill-background" : "fill-foreground"}
            />
            <path
                d="M10 16.5c1.6-4 6.2-5.6 10.2-3.6 1.6-2.6 5.4-3.4 8.2-1.2 1.6-.4 3.6.6 4.1 2.4"
                className={inverted ? "stroke-foreground" : "stroke-background"}
                strokeWidth="1.8"
                strokeLinecap="round"
                fill="none"
            />
            <path
                d="M12 23h16M12 27.5h11"
                className={inverted ? "stroke-foreground" : "stroke-background"}
                strokeWidth="1.8"
                strokeLinecap="round"
            />
        </svg>
    )
}
