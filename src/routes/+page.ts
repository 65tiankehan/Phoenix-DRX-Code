import { redirect } from '@sveltejs/kit'

// 根路径重定向到 01 服务器选择屏（旧路由 / → /server-select 语义平移）
export function load() {
  redirect(307, '/server-select')
}
