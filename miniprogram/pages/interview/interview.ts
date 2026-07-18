import {
  continueInterview,
  ensureLogin,
  finishInterview,
  generateChapter,
  GeneratedChapter,
  InterviewMessage,
  listMessages,
  postMessage,
  startInterview,
} from '../../services/api'

// Pure clear helper (also unit-tested via require of the .js file).
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { afterSuccessfulSend } = require('../../utils/composer_clear') as {
  afterSuccessfulSend: (s: {
    draft: string
    composerKey: number
    success: boolean
  }) => { draft: string; composerKey: number; shouldRemount: boolean }
}

Page({
  data: {
    memoirId: '',
    sessionId: '',
    topic: '童年与家庭',
    messages: [] as InterviewMessage[],
    composerKey: 0,
    /** false briefly after send to destroy native textarea, then remount empty */
    composerAlive: true,
    sending: false,
    generating: false,
    finished: false,
    resumed: false,
    userTurnCount: 0,
    autoGenerateAt: 20,
    saveHint: '对话将实时保存',
    generatedPreview: false,
    error: '',
    scrollInto: '',
  },

  _draft: '',
  _lastGenerated: null as GeneratedChapter | null,

  async onLoad(query: Record<string, string | undefined>) {
    const memoirId = query.memoirId || ''
    const mode = (query.mode || 'continue') as 'start' | 'continue'
    const sessionId = query.sessionId || ''
    const topic = decodeURIComponent(query.topic || '童年与家庭')
    this.setData({ memoirId, topic })
    if (!memoirId) {
      this.setData({ error: '缺少回忆录 ID' })
      return
    }
    try {
      await ensureLogin()
      if (mode === 'continue' && sessionId) {
        const continued = await continueInterview(sessionId)
        this.applySession(continued, true)
        await this.reloadMessages()
        wx.showToast({ title: '已恢复历史对话', icon: 'none' })
        return
      }
      if (mode === 'start') {
        const started = await startInterview(memoirId, topic, { forceNew: true })
        this.applySession(started, false)
        await this.reloadMessages()
        return
      }
      const resumed = await startInterview(memoirId, topic, { forceNew: false })
      this.applySession(resumed, !!resumed.resumed)
      await this.reloadMessages()
      if (resumed.resumed) {
        wx.showToast({ title: '已恢复历史对话', icon: 'none' })
      }
    } catch (e: any) {
      this.setData({ error: e?.message || '无法打开采访' })
    }
  },

  applySession(
    started: {
      session: { id: string; status: string }
      resumed?: boolean
      user_turn_count?: number
      auto_generate_at?: number
    },
    resumed: boolean,
  ) {
    this.setData({
      sessionId: started.session.id,
      resumed,
      userTurnCount: started.user_turn_count || 0,
      autoGenerateAt: started.auto_generate_at || 20,
      finished: started.session.status === 'finished',
      saveHint: resumed ? '已恢复历史对话' : '对话已写入数据库',
    })
  },

  async reloadMessages() {
    const sessionId = this.data.sessionId
    if (!sessionId) return
    const messages = await listMessages(sessionId)
    const last = messages[messages.length - 1]
    const userTurnCount = messages.filter((m) => m.role === 'user').length
    this.setData({
      messages,
      userTurnCount,
      scrollInto: last ? `m-${last.id}` : '',
      saveHint: `已保存 ${messages.length} 条`,
    })
  },

  onDraft(e: WechatMiniprogram.Input) {
    this._draft = e.detail.value || ''
  },

  /**
   * Empty draft + destroy/remount native textarea so prior text cannot linger.
   * Only call after a successful send.
   */
  clearComposerAfterSuccess() {
    const next = afterSuccessfulSend({
      draft: this._draft,
      composerKey: this.data.composerKey,
      success: true,
    })
    this._draft = next.draft
    // Step 1: unmount so the native control cannot keep old composition text.
    this.setData({
      composerAlive: false,
      composerKey: next.composerKey,
    })
    // Step 2: remount empty input on next tick.
    setTimeout(() => {
      this.setData({ composerAlive: true })
    }, 16)
  },

  async onSend() {
    const content = (this._draft || '').trim()
    if (!content) {
      this.setData({ error: '请先输入内容' })
      return
    }
    const ok = await this.sendWithAction(content, 'normal')
    if (ok) {
      this.clearComposerAfterSuccess()
    }
  },

  async onAction(e: WechatMiniprogram.TouchEvent) {
    const action = e.currentTarget.dataset.action as
      | 'dont_know'
      | 'change_question'
      | 'prefer_not'
      | 'end'
    if (action === 'end') {
      await this.endSession()
      return
    }
    await this.sendWithAction('', action)
  },

  /** @returns true when the message was accepted by the server */
  async sendWithAction(
    content: string,
    action: 'normal' | 'dont_know' | 'change_question' | 'prefer_not' | 'end',
  ): Promise<boolean> {
    if (!this.data.sessionId || this.data.finished) return false
    this.setData({ sending: true, error: '', saveHint: '正在保存…' })
    try {
      const resp = await postMessage(this.data.sessionId, content, action)
      if (resp.session_status === 'finished') {
        this.setData({ finished: true })
      }
      this.setData({
        userTurnCount: resp.user_turn_count ?? this.data.userTurnCount,
        autoGenerateAt: resp.auto_generate_at || this.data.autoGenerateAt,
        saveHint: '已保存到数据库',
      })
      await this.reloadMessages()
      if (resp.generated) {
        this.onGenerated(resp.generated)
      }
      return true
    } catch (e: any) {
      this.setData({ error: e?.message || '发送失败', saveHint: '保存失败，请重试' })
      return false
    } finally {
      this.setData({ sending: false })
    }
  },

  async onGenerate() {
    if (!this.data.sessionId || this.data.generating) return
    if (this.data.userTurnCount < 1) {
      this.setData({ error: '请先回答几个问题，再生成回忆录' })
      return
    }
    this.setData({ generating: true, error: '' })
    try {
      const generated = await generateChapter(this.data.sessionId)
      this.onGenerated(generated)
      wx.showToast({ title: '章节已生成', icon: 'success' })
    } catch (e: any) {
      this.setData({ error: e?.message || '生成失败' })
    } finally {
      this.setData({ generating: false })
    }
  },

  onGenerated(generated: GeneratedChapter) {
    this._lastGenerated = generated
    this.setData({
      generatedPreview: true,
      saveHint:
        generated.trigger === 'auto'
          ? `已满${generated.user_turn_count}轮，自动生成并保存`
          : '章节草稿已写入数据库',
    })
    if (generated.trigger === 'auto') {
      wx.showModal({
        title: '已自动生成章节',
        content: `本次对话已达 ${generated.user_turn_count} 轮，章节「${generated.chapter.title}」草稿已保存。是否现在查看？`,
        confirmText: '查看',
        success: (res) => {
          if (res.confirm) this.onViewGenerated()
        },
      })
    }
  },

  onViewGenerated() {
    const g = this._lastGenerated
    const memoirId = this.data.memoirId
    if (g?.chapter?.id) {
      wx.navigateTo({
        url: `/pages/reader/reader?memoirId=${memoirId}&chapterId=${g.chapter.id}`,
      })
      return
    }
    wx.navigateTo({
      url: `/pages/reader/reader?memoirId=${memoirId}`,
    })
  },

  async endSession() {
    if (!this.data.sessionId) return
    this.setData({ sending: true, error: '' })
    try {
      await postMessage(this.data.sessionId, '结束本次采访', 'end')
      await finishInterview(this.data.sessionId)
      this.setData({ finished: true, saveHint: '采访已结束，记录已保存' })
      await this.reloadMessages()
    } catch (e: any) {
      this.setData({ error: e?.message || '结束失败' })
    } finally {
      this.setData({ sending: false })
    }
  },
})
