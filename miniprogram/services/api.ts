import { API_BASE_URL } from '../config/env'

const TOKEN_KEY = 'memoir_token'
const USER_KEY = 'memoir_user'

export function getToken(): string {
  return wx.getStorageSync(TOKEN_KEY) || ''
}

export function setToken(token: string) {
  wx.setStorageSync(TOKEN_KEY, token)
}

export function clearToken() {
  wx.removeStorageSync(TOKEN_KEY)
  wx.removeStorageSync(USER_KEY)
}

export function getCachedUser(): AuthResponse | null {
  try {
    return wx.getStorageSync(USER_KEY) || null
  } catch {
    return null
  }
}

function setCachedUser(user: AuthResponse) {
  wx.setStorageSync(USER_KEY, user)
}

type Method = 'GET' | 'POST' | 'PATCH' | 'DELETE'

interface RequestOptions {
  path: string
  method?: Method
  data?: WechatMiniprogram.IAnyObject | string
  auth?: boolean
  /** ms; LLM-backed routes need longer than the WeChat default (~60s). */
  timeout?: number
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
      // Default 90s; interview/generate can still take tens of seconds even after caps.
      timeout: options.timeout ?? 90000,
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
  avatar_url?: string
}

export interface UserProfile {
  id: string
  nickname: string
  avatar_url?: string
  wechat_openid?: string
  created_at: string
}

/** wx.login → backend /auth/wechat (real openid exchange; no mock). */
export function wechatLogin(profile?: {
  nickname?: string
  avatar_url?: string
}): Promise<AuthResponse> {
  return new Promise((resolve, reject) => {
    wx.login({
      success: async (res) => {
        if (!res.code) {
          reject(new Error('微信登录失败：未获取到 code'))
          return
        }
        try {
          const auth = await request<AuthResponse>({
            path: '/auth/wechat',
            method: 'POST',
            auth: false,
            data: {
              code: res.code,
              nickname: profile?.nickname,
              avatar_url: profile?.avatar_url,
            },
          })
          setToken(auth.token)
          setCachedUser(auth)
          resolve(auth)
        } catch (e: any) {
          reject(e instanceof Error ? e : new Error(String(e)))
        }
      },
      fail: (err) => reject(new Error(err.errMsg || 'wx.login 调用失败')),
    })
  })
}

export async function getMe(): Promise<UserProfile> {
  return request<UserProfile>({ path: '/me' })
}

export async function updateProfile(data: {
  nickname?: string
  avatar_url?: string
}): Promise<UserProfile> {
  const me = await request<UserProfile>({ path: '/me', method: 'PATCH', data })
  const cached = getCachedUser()
  if (cached) {
    setCachedUser({
      ...cached,
      nickname: me.nickname,
      avatar_url: me.avatar_url,
      user_id: me.id,
    })
  }
  return me
}

const LOGIN_CACHE_MS = 10 * 60 * 1000
let _loginCache: { at: number; auth: AuthResponse } | null = null

/**
 * Ensure a valid session: reuse token if /me works, otherwise real WeChat login.
 * Caches successful validation for LOGIN_CACHE_MS to avoid /me on every page.
 */
export async function ensureLogin(): Promise<AuthResponse> {
  const existing = getToken()
  if (existing && _loginCache && _loginCache.auth.token === existing) {
    if (Date.now() - _loginCache.at < LOGIN_CACHE_MS) {
      return _loginCache.auth
    }
  }
  if (existing) {
    try {
      const me = await getMe()
      const auth: AuthResponse = {
        token: existing,
        user_id: me.id,
        nickname: me.nickname,
        avatar_url: me.avatar_url,
      }
      setCachedUser(auth)
      _loginCache = { at: Date.now(), auth }
      return auth
    } catch {
      clearToken()
      _loginCache = null
    }
  }
  const auth = await wechatLogin()
  _loginCache = { at: Date.now(), auth }
  return auth
}

export function isLoggedIn(): boolean {
  return !!getToken()
}

export function logout() {
  clearToken()
  _loginCache = null
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
  /** Present on list endpoint: whether any interview session exists. */
  has_interview?: boolean
  message_count?: number
  continue_session_id?: string
  continue_topic?: string
}

export interface Chapter {
  id: string
  memoir_id: string
  title: string
  sort_order: number
  status: string
  summary?: string
  content?: string
  /** Present on list chapters: interview progress for this chapter/topic. */
  message_count?: number
  continue_session_id?: string
  has_interview?: boolean
  has_draft?: boolean
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

export function listChapters(
  memoirId: string,
  options?: { includeContent?: boolean },
) {
  const include = options?.includeContent ? 'true' : 'false'
  return request<Chapter[]>({
    path: `/memoirs/${memoirId}/chapters?include_content=${include}`,
  })
}

/** Delete memoir and cascaded chapters / interviews / messages. */
export function deleteMemoir(id: string) {
  return request<void>({ path: `/memoirs/${id}`, method: 'DELETE' })
}

export interface InterviewSession {
  id: string
  memoir_id: string
  chapter_id?: string
  topic: string
  status: string
  summary?: string
  auto_generated_at?: string
}

export interface InterviewMessage {
  id: string
  session_id: string
  role: 'user' | 'assistant' | 'system' | string
  content: string
  question_type?: string
}

export interface GeneratedChapter {
  chapter: Chapter
  session_summary?: string
  trigger: string
  user_turn_count: number
}

export interface StartInterviewResult {
  session: InterviewSession
  opening_message?: InterviewMessage
  resumed: boolean
  user_turn_count: number
  auto_generate_at: number
}

export interface PostMessageResult {
  user_message: InterviewMessage
  assistant_message?: InterviewMessage
  session_status: string
  user_turn_count: number
  auto_generate_at: number
  generated?: GeneratedChapter
  generation_started?: boolean
  generation_status?: string
}

/** First-time start only. Use forceNew when you intentionally want a fresh session. */
export function startInterview(
  memoirId: string,
  topic: string,
  options?: { chapterId?: string; forceNew?: boolean },
) {
  return request<StartInterviewResult>({
    path: `/memoirs/${memoirId}/interviews`,
    method: 'POST',
    data: {
      topic,
      chapter_id: options?.chapterId || undefined,
      force_new: options?.forceNew ?? false,
    },
  })
}

/** Continue an existing session by id — never creates a new empty chat. */
export function continueInterview(sessionId: string) {
  return request<StartInterviewResult>({
    path: `/interviews/${sessionId}/continue`,
    method: 'POST',
    data: {},
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
  return request<PostMessageResult>({
    path: `/interviews/${sessionId}/messages`,
    method: 'POST',
    data: { content, action },
    timeout: 120000,
  })
}

export function finishInterview(sessionId: string) {
  return request<InterviewSession>({
    path: `/interviews/${sessionId}/finish`,
    method: 'POST',
    data: {},
  })
}

/** Generate chapter draft from session transcript and save to chapters.content. */
export function generateChapter(sessionId: string) {
  return request<GeneratedChapter>({
    path: `/interviews/${sessionId}/generate`,
    method: 'POST',
    data: {},
    timeout: 120000,
  })
}
