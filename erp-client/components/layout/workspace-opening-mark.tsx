"use client"

import "./workspace-opening-mark.css"

/** 工作区 iframe 未就绪时的桌面窗口描边。纯装饰，文案由外层 status 读出。 */
export function WorkspaceOpeningMark() {
    return (
        <svg
            viewBox="0 0 56 36"
            aria-hidden="true"
            focusable="false"
            className="workspace-opening-mark h-9 w-14 text-foreground/80"
        >
            <rect
                x="1.5"
                y="1.5"
                width="53"
                height="33"
                rx="2"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.25"
                opacity="0.4"
            />
            <path d="M15 1.5v33" className="workspace-opening-chrome" />
            <path d="M15 9.5h39.5" className="workspace-opening-chrome" />
            <path
                d="M20 16h22"
                className="workspace-opening-line workspace-opening-line-a"
            />
            <path
                d="M20 22h14"
                className="workspace-opening-line workspace-opening-line-b"
            />
            <path
                d="M20 28h18"
                className="workspace-opening-line workspace-opening-line-c"
            />
        </svg>
    )
}
