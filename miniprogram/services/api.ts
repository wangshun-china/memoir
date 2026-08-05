import { API_BASE_URL } from '../config/env'
import { Utf8Decoder } from '../utils/sse'

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
      timeout: (options.timeout != null ? options.timeout : 90000),
      success(res) {
        if (res.statusCode >= 200 && res.statusCode < 300) {
          resolve(res.data as T)
        } else {
          const errBody = res.data as { error?: string }
          reject(new Error((errBody && errBody.error) || `HTTP ${res.statusCode}`))
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
  username?: string
  is_admin?: boolean
  registered?: boolean
}

export interface UserProfile {
  id: string
  nickname: string
  avatar_url?: string
  wechat_openid?: string
  username?: string
  is_admin?: boolean
  created_at: string
}

function persistAuth(auth: AuthResponse) {
  setToken(auth.token)
  setCachedUser(auth)
  _loginCache = { at: Date.now(), auth }
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
              nickname: profile && profile.nickname,
              avatar_url: profile && profile.avatar_url,
            },
          })
          persistAuth(auth)
          resolve(auth)
        } catch (e: any) {
          reject(e instanceof Error ? e : new Error(String(e)))
        }
      },
      fail: (err) => reject(new Error(err.errMsg || 'wx.login 调用失败')),
    })
  })
}

/**
 * Account + password: if username not found → auto-register; else login.
 * Username `wangshun` is granted admin by the server.
 */
export async function passwordLogin(
  username: string,
  password: string,
): Promise<AuthResponse> {
  const auth = await request<AuthResponse>({
    path: '/auth/password',
    method: 'POST',
    auth: false,
    data: { username: username.trim(), password },
  })
  persistAuth(auth)
  return auth
}

/**
 * Forgot password: recovery_key must be `wangshun` (trial).
 * Does not log the user in — call passwordLogin after success.
 */
export async function resetPassword(
  username: string,
  recoveryKey: string,
  newPassword: string,
): Promise<{ ok: boolean; message?: string }> {
  return request({
    path: '/auth/reset-password',
    method: 'POST',
    auth: false,
    data: {
      username: username.trim(),
      recovery_key: recoveryKey.trim(),
      new_password: newPassword,
    },
  })
}

export async function getMe(): Promise<UserProfile> {
  return request<UserProfile>({ path: '/me' })
}

// --- Admin (requires users.is_admin or legacy admin JWT) ---

export interface AdminOverview {
  users: number
  memoirs: number
  interview_sessions: number
  interview_messages: number
  llm_calls: number
  llm_tokens: number
  llm_success_rate: number
  ai: {
    api_base: string
    api_key_set: boolean
    api_key_masked: string
    model: string
    mode: string
    has_live_client: boolean
    enable_thinking: boolean
  }
}

export interface AdminUserRow {
  id: string
  nickname: string
  username?: string
  wechat_openid?: string
  role: string
  is_admin: boolean
  memoir_count: number
  created_at: string
}

export interface AdminMemoirRow {
  id: string
  title: string
  subject_name: string
  status: string
  creator_nickname: string
  chapter_count: number
  message_count: number
  created_at: string
}

export async function adminOverview(): Promise<AdminOverview> {
  return request({ path: '/admin/overview' })
}

export async function adminUsers(): Promise<AdminUserRow[]> {
  return request({ path: '/admin/users' })
}

export async function adminMemoirs(): Promise<AdminMemoirRow[]> {
  return request({ path: '/admin/memoirs' })
}

export async function adminAiConfig(): Promise<AdminOverview['ai']> {
  return request({ path: '/admin/ai-config' })
}

export async function adminPutAiConfig(data: {
  api_base?: string
  api_key?: string
  model?: string
  clear_api_key?: boolean
  enable_thinking?: boolean
}): Promise<AdminOverview['ai']> {
  return request({ path: '/admin/ai-config', method: 'PUT', data })
}

export async function adminTestAi(prompt?: string): Promise<{
  ok: boolean
  reply: string
  model: string
  error?: string
  latency_ms: number
}> {
  return request({
    path: '/admin/ai-config/test',
    method: 'POST',
    data: { prompt: prompt || '请用一句话自我介绍你是回忆录采访助手。' },
    timeout: 60000,
  })
}

export async function adminBindWechat(code: string): Promise<{
  bound: boolean
  user_id: string
  openid_masked: string
  promoted: boolean
}> {
  return request({
    path: '/admin/bind-wechat',
    method: 'POST',
    data: { code },
    timeout: 30000,
  })
}

export async function adminAiUsage(limit = 20): Promise<{
  summary: {
    calls: number
    success_calls: number
    total_tokens: number
    avg_latency_ms: number
  }
  recent: Array<{
    source: string
    model: string
    total_tokens: number
    success: boolean
    latency_ms: number
    error_message?: string
    created_at: string
  }>
}> {
  return request({ path: '/admin/ai-usage?limit=' + limit })
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
        username: me.username,
        is_admin: !!me.is_admin,
      }
      setCachedUser(auth)
      _loginCache = { at: Date.now(), auth }
      return auth
    } catch {
      clearToken()
      _loginCache = null
    }
  }
  // No valid session: pages should show login UI (password or WeChat). Do not force WeChat.
  throw new Error('请先登录')
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
  const include = (options && options.includeContent) ? 'true' : 'false'
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
      chapter_id: (options && options.chapterId) || undefined,
      force_new: ((options && options.forceNew) != null ? !!(options && options.forceNew) : false),
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

export interface StreamCallbacks {
  onThinking?: (text: string) => void
  onContent?: (text: string) => void
}

interface StreamEventPayload {
  kind: 'thinking' | 'content' | 'done' | 'error'
  text?: string
  message?: string
  response?: PostMessageResult
}

/**
 * Stream a message to the interviewer over SSE (`POST .../messages/stream`).
 * The server emits `data: {kind:...}` frames: thinking → content → done.
 * Resolves with the same `PostMessageResult` as `postMessage` once the server
 * persists the assistant reply. Falls back to parsing the full body when the
 * base library does not support `enableChunked`.
 */
export function postMessageStream(
  sessionId: string,
  content: string,
  action: 'normal' | 'dont_know' | 'change_question' | 'prefer_not' = 'normal',
  callbacks?: StreamCallbacks,
): Promise<PostMessageResult> {
  return new Promise((resolve, reject) => {
    let settled = false
    const done = (result: PostMessageResult) => {
      if (!settled) {
        settled = true
        resolve(result)
      }
    }
    const fail = (err: Error) => {
      if (!settled) {
        settled = true
        reject(err)
      }
    }

    // Buffer persists across TCP chunks; SSE frames are "\n\n"-delimited.
    let buffer = ''
    const handleText = (text: string) => {
      buffer += text
      let idx = buffer.indexOf('\n\n')
      while (idx >= 0) {
        const frame = buffer.slice(0, idx)
        buffer = buffer.slice(idx + 2)
        const line = frame.split('\n').find((l) => l.trim().startsWith('data:'))
        if (line) {
          const payload = line.slice(5).trim()
          if (payload && payload !== '[DONE]') {
            let evt: StreamEventPayload
            try {
              evt = JSON.parse(payload)
            } catch {
              // Incomplete JSON frame — skip; next chunk may complete it.
              idx = buffer.indexOf('\n\n')
              continue
            }
            if (evt.kind === 'thinking' && evt.text && callbacks && callbacks.onThinking) {
              callbacks.onThinking(evt.text)
            } else if (evt.kind === 'content' && evt.text && callbacks && callbacks.onContent) {
              callbacks.onContent(evt.text)
            } else if (evt.kind === 'done' && evt.response) {
              done(evt.response)
              return
            } else if (evt.kind === 'error') {
              fail(new Error(evt.message || 'stream error'))
              return
            }
          }
        }
        idx = buffer.indexOf('\n\n')
      }
    }

    const token = getToken()
    const header: Record<string, string> = {
      'content-type': 'application/json',
      Accept: 'text/event-stream',
    }
    if (token) header.Authorization = `Bearer ${token}`

    const task = wx.request({
      url: `${API_BASE_URL}/interviews/${sessionId}/messages/stream`,
      method: 'POST',
      data: { content, action },
      header,
      enableChunked: true,
      success(res) {
        // Older base libs ignore enableChunked; the full SSE body arrives here.
        if (!settled && typeof res.data === 'string') {
          handleText(res.data)
        }
      },
      fail(err) {
        fail(new Error(err.errMsg || 'network error'))
      },
    })

    const chunked = task as WechatMiniprogram.RequestTask & {
      onChunkReceived?: (cb: (res: { data: ArrayBuffer }) => void) => void
    }
    if (chunked && typeof chunked.onChunkReceived === 'function') {
      const decoder = new Utf8Decoder()
      chunked.onChunkReceived((res: { data: ArrayBuffer }) => {
        const u8 = new Uint8Array(res.data)
        handleText(decoder.push(u8))
      })
    }
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

export interface StoryCard {
  id: string
  memoir_id: string
  session_id?: string
  title: string
  summary: string
  narrative: string
  life_stage?: string
  time_text?: string
  year_start?: number
  year_end?: number
  time_precision: 'exact' | 'approximate' | 'range' | 'unknown' | string
  location_text?: string
  people: string[]
  themes: string[]
  emotions: string[]
  missing_details: string[]
  primary_chapter_id?: string
  primary_chapter_title?: string
  status: 'draft' | 'confirmed' | string
  source_count: number
}

export interface TimelineEvent {
  story_id: string
  title: string
  time_text?: string
  year_start?: number
  year_end?: number
  time_precision: string
  summary: string
}

export interface OrganizeResult {
  timeline: TimelineEvent[]
  chapters: Chapter[]
  story_count: number
}

/** Turn one short interview into an idempotent, source-linked story card. */
export function extractStory(sessionId: string) {
  return request<StoryCard>({
    path: `/interviews/${sessionId}/stories`,
    method: 'POST',
    data: {},
    timeout: 120000,
  })
}

export function listStories(memoirId: string) {
  return request<StoryCard[]>({ path: `/memoirs/${memoirId}/stories` })
}

export function confirmStory(storyId: string) {
  return request<StoryCard>({
    path: `/stories/${storyId}/confirm`,
    method: 'POST',
    data: {},
  })
}

export function organizeStories(memoirId: string) {
  return request<OrganizeResult>({
    path: `/memoirs/${memoirId}/organize`,
    method: 'POST',
    data: {},
    timeout: 120000,
  })
}
