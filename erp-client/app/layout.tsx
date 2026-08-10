import type { Metadata } from "next"
import { Suspense } from "react"
import { Noto_Sans } from "next/font/google"
import "./globals.css"
import { cn } from "@/lib/utils"
import { QueryProvider } from "@/components/providers/query-provider"
import { AuthSessionProvider } from "@/components/providers/auth-session-provider"
import { Toaster } from "@/components/ui/toast"

const notoSans = Noto_Sans({ subsets: ["latin"], variable: "--font-sans" })

export const metadata: Metadata = {
  title: {
    default: "福尚云 ERP",
    template: "%s · 福尚云 ERP",
  },
  description: "福尚云 ERP 业务记录、单据流转与经营协同平台",
}

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode
}>) {
  return (
    <html
      lang="zh-CN"
      className={cn("h-full", "antialiased", "font-sans", notoSans.variable)}
      suppressHydrationWarning
    >
      <body className="min-h-full flex flex-col">
        <QueryProvider>
          <Suspense fallback={null}>
            <AuthSessionProvider>{children}</AuthSessionProvider>
          </Suspense>
        </QueryProvider>
        <Toaster />
      </body>
    </html>
  )
}
