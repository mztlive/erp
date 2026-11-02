"use client"

import { LoginForm } from "../components/login-form"
import { LoginLayout } from "../components/login-layout"

/**
 * 后台登录页（公开路由）：左品牌舞台 + 右登录表单。
 */
export function LoginPage() {
    return (
        <LoginLayout mode="admin">
            <LoginForm />
        </LoginLayout>
    )
}
