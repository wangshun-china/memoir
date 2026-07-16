import { API_BASE_URL } from '../config/env'

const TOKEN_KEY = 'memoir_token'

export function getToken(): string {
  return wx.getStorageSync(TOKEN_KEY) || ''
}

export function setToken(token: string) {
  wx.setStorageSync(TOKEN_KEY, token)
}

export function clearToken() {
  wx.removeStorageSync(TOKEN_KEY)
}

type Method = 'GET' | 'POST' | 'PATCH' | 'DELETE'

interface RequestOptions {
  path: string
  method?: Method
  data?: WechatMiniprogram.IAnyObject | string
  auth?: boolean
}

export function request<T = WechatMiniprogram.IAnyObject>(options: RequestOptions): Promise<T> {
  const method = options.method || 'GET'
  const auth = options.auth !== false
  const header: Record<string, string> = {
    'content-type': 'application/json',
  }
  if (auth) {
    const token = getToken()
    if (token) {
      header.Authorization = `Bearer ${token}`
    }
  }

  return new Promise((resolve, reject) => {
    wx.request({
      url: `${API_BASE_URL}${options.path}`,
      method,
      data: options.data,
      header,
      success(res) {
        if (res.statusCode >= 200 && res.statusCode < 300) {
          resolve(res.data as T)
        } else {
          const errBody = res.data as { error?: string }
          reject(new Error(errBody?.error || `HTTP ${res.statusCode}`))
        }
      },
      fail(err) {
        reject(new Error(err.errMsg || 'network error'))
      },
    })
  })
}

export interface AuthResponse {
  token: string
  user_id: string
  nickname: string
}

export async function ensureLogin(): Promise<AuthResponse> {
  const existing = getToken()
  if (existing) {
    return { token: existing, user_id: '', nickname: '' }
  }
  // Stage-1: dev mock user. Production will use wx.login + /auth/wechat.
  const auth = await request<AuthResponse>({
    path: '/auth/dev-login',
    method: 'POST',
    auth: false,
    data: { nickname: '小程序用户', openid: 'mp-dev-user' },
  })
  setToken(auth.token)
  return auth
}

export interface Memoir {
  id: string
  title: string
  subject_name: string
  birth_year?: number
  birth_place?: string
  preferred_name?: string
  creator_relation?: string
  status: string
}

export interface Chapter {
  id: string
  memoir_id: string
  title: string
  sort_order: number
  status: string
}

export interface MemoirWithChapters extends Memoir {
  chapters: Chapter[]
}

export function listMemoirs() {
  return request<Memoir[]>({ path: '/memoirs' })
}

export function createMemoir(data: {
  subject_name: string
  title?: string
  birth_year?: number
  birth_place?: string
  preferred_name?: string
  creator_relation?: string
}) {
  return request<MemoirWithChapters>({ path: '/memoirs', method: 'POST', data })
}

export function getMemoir(id: string) {
  return request<Memoir>({ path: `/memoirs/${id}` })
}

export function listChapters(memoirId: string) {
  return request<Chapter[]>({ path: `/memoirs/${memoirId}/chapters` })
}

export interface InterviewSession {
  id: string
  memoir_id: string
  chapter_id?: string
  topic: string
  status: string
}

export interface InterviewMessage {
  id: string
  session_id: string
  role: 'user' | 'assistant' | 'system' | string
  content: string
  question_type?: string
}

export function startInterview(memoirId: string, topic: string, chapterId?: string) {
  return request<{ session: InterviewSession; opening_message: InterviewMessage }>({
    path: `/memoirs/${memoirId}/interviews`,
    method: 'POST',
    data: { topic, chapter_id: chapterId },
  })
}

export function listMessages(sessionId: string) {
  return request<InterviewMessage[]>({ path: `/interviews/${sessionId}/messages` })
}

export function postMessage(
  sessionId: string,
  content: string,
  action: 'normal' | 'dont_know' | 'change_question' | 'prefer_not' | 'end' = 'normal',
) {
  return request<{
    user_message: InterviewMessage
    assistant_message?: InterviewMessage
    session_status: string
  }>({
    path: `/interviews/${sessionId}/messages`,
    method: 'POST',
    data: { content, action },
  })
}

export function finishInterview(sessionId: string) {
  return request<InterviewSession>({
    path: `/interviews/${sessionId}/finish`,
    method: 'POST',
    data: {},
  })
}
