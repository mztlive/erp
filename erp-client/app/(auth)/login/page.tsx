import { LoginPage } from "@/features/auth/login-page"
import { RedirectIfAuthenticated } from "@/components/providers/auth-session-provider"

/**
 * 登录路由：已登录则跳转 returnTo / 工作台。
 */
export default function LoginRoutePage() {
  return (
    <RedirectIfAuthenticated>
      <LoginPage />
    </RedirectIfAuthenticated>
  )
}
