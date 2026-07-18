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

// eslint-disable-next-line @typescript-eslint/no-require-imports
const { afterSuccessfulSend } = require('../../utils/composer_clear') as {
  afterSuccessfulSend: (s: {
    draft: string
    composerKey: number
    success: boolean
  }) => { draft: string; composerKey: number; shouldRemount: boolean }
}

/** Short subtitle under chapter title (UI only; server has full opening hook). */
function topicHintFor(topic: string): string {
  const map: Record<string, string> = {
    童年与家庭: '小时候住过的家',
    求学经历: '学校与读书的日子',
    青年时代: '青春里的选择与成长',
    工作与事业: '岗位上的故事',
    婚姻与家庭: '成家与相伴',
    人生转折: '改变命运的那几步',
    子女与家庭生活: '和孩子在一起的日子',
    退休与晚年: '晚年的日常与心境',
    我想留下的话: '最想留给家人的话',
  }
  return map[topic] || '慢慢说，想到什么说什么'
}

Page({
  data: {
    memoirId: '',
    sessionId: '',
    topic: '童年与家庭',
    topicHint: '小时候住过的家',
    chapterId: '',
    messages: [] as InterviewMessage[],
    composerKey: 0,
    composerAlive: true,
    sending: false,
    generating: false,
    finished: false,
    resumed: false,
    userTurnCount: 0,
    autoGenerateAt: 20,
    saveHint: '对话将实时保存',
    waitHint: '',
    generatedPreview: false,
    generationStatus: '',
    error: '',
    scrollInto: '',
  },

  _draft: '',
  _lastGenerated: null as GeneratedChapter | null,
  _tempId: 0,

  async onLoad(query: Record<string, string | undefined>) {
    const memoirId = query.memoirId || ''
    const mode = (query.mode || 'continue') as 'start' | 'continue'
    const sessionId = query.sessionId || ''
    const chapterId = query.chapterId || ''
    const topic = decodeURIComponent(query.topic || '童年与家庭')
    const topicHint = topicHintFor(topic)
    this.setData({ memoirId, topic, topicHint, chapterId })
    if (!memoirId) {
      this.setData({ error: '缺少回忆录 ID' })
      return
    }
    try {
      wx.showLoading({ title: '加载中…', mask: true })
      await ensureLogin()
      if (mode === 'continue' && sessionId) {
        const continued = await continueInterview(sessionId)
        this.applySession(continued, true)
        await this.reloadMessages()
        wx.showToast({ title: '已恢复历史对话', icon: 'none' })
        return
      }
      if (mode === 'start') {
        const started = await startInterview(memoirId, topic, {
          forceNew: true,
          chapterId: chapterId || undefined,
        })
        this.applySession(started, false)
        await this.reloadMessages()
        return
      }
      // Fallback continue without sessionId — backend resumes by memoir+topic
      const resumed = await startInterview(memoirId, topic, {
        forceNew: false,
        chapterId: chapterId || undefined,
      })
      this.applySession(resumed, !!resumed.resumed)
      await this.reloadMessages()
      if (resumed.resumed) {
        wx.showToast({ title: '已恢复历史对话', icon: 'none' })
      }
    } catch (e: any) {
      this.setData({ error: (e && e.message) || '无法打开采访' })
    } finally {
      wx.hideLoading()
    }
  },

  applySession(
    started: {
      session: { id: string; status: string; generation_status?: string }
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
      generationStatus: started.session.generation_status || '',
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

  appendMessages(extra: InterviewMessage[]) {
    const messages = this.data.messages.concat(extra)
    const last = messages[messages.length - 1]
    const userTurnCount = messages.filter((m) => m.role === 'user').length
    this.setData({
      messages,
      userTurnCount,
      scrollInto: last ? `m-${last.id}` : '',
    })
  },

  onDraft(e: WechatMiniprogram.Input) {
    this._draft = e.detail.value || ''
  },

  clearComposerAfterSuccess() {
    const next = afterSuccessfulSend({
      draft: this._draft,
      composerKey: this.data.composerKey,
      success: true,
    })
    this._draft = next.draft
    this.setData({
      composerAlive: false,
      composerKey: next.composerKey,
    })
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

  /** 次要操作：生成 / 不想说 / 结束 — 避免底栏过满 */
  onMoreTools() {
    if (this.data.finished || this.data.sending) return
    const items = ['生成回忆录', '这个问题不想说', '结束本次采访']
    wx.showActionSheet({
      itemList: items,
      success: async (res) => {
        if (res.tapIndex === 0) {
          await this.onGenerate()
        } else if (res.tapIndex === 1) {
          await this.sendWithAction('', 'prefer_not')
        } else if (res.tapIndex === 2) {
          await this.endSession()
        }
      },
    })
  },

  async sendWithAction(
    content: string,
    action: 'normal' | 'dont_know' | 'change_question' | 'prefer_not' | 'end',
  ): Promise<boolean> {
    if (!this.data.sessionId || this.data.finished || this.data.sending) return false

    // Optimistic user bubble for normal text
    let optimisticId = ''
    if (action === 'normal' && content) {
      this._tempId += 1
      optimisticId = `tmp-${this._tempId}`
      const optimistic: InterviewMessage = {
        id: optimisticId,
        session_id: this.data.sessionId,
        role: 'user',
        content,
      }
      this.appendMessages([optimistic])
    }

    this.setData({
      sending: true,
      error: '',
      saveHint: '正在保存…',
      waitHint: '正在听你说，整理下一问…',
    })
    try {
      const resp = await postMessage(this.data.sessionId, content, action)
      if (resp.session_status === 'finished') {
        this.setData({ finished: true })
      }

      // Replace optimistic / append server messages without full refetch
      let messages = this.data.messages.filter((m) => m.id !== optimisticId)
      if (resp.user_message) {
        const has = messages.some((m) => m.id === resp.user_message.id)
        if (!has) messages = messages.concat([resp.user_message])
      }
      if (resp.assistant_message) {
        const has = messages.some((m) => m.id === resp.assistant_message!.id)
        if (!has) messages = messages.concat([resp.assistant_message])
      }
      const last = messages[messages.length - 1]
      this.setData({
        messages,
        userTurnCount: (resp.user_turn_count != null ? resp.user_turn_count : this.data.userTurnCount),
        autoGenerateAt: resp.auto_generate_at || this.data.autoGenerateAt,
        saveHint: '已保存到数据库',
        waitHint: '',
        generationStatus: resp.generation_status || this.data.generationStatus,
        scrollInto: last ? `m-${last.id}` : '',
      })

      if (resp.generation_started) {
        this.setData({
          generatedPreview: true,
          saveHint: '章节正在后台生成…',
          generationStatus: 'generating',
        })
        wx.showToast({ title: '已开始生成章节', icon: 'none' })
      }
      if (resp.generated) {
        this.onGenerated(resp.generated)
      }
      return true
    } catch (e: any) {
      // Drop optimistic bubble on failure
      if (optimisticId) {
        this.setData({
          messages: this.data.messages.filter((m) => m.id !== optimisticId),
        })
      }
      this.setData({
        error: (e && e.message) || '发送失败',
        saveHint: '保存失败，请重试',
        waitHint: '',
      })
      return false
    } finally {
      this.setData({ sending: false, waitHint: '' })
    }
  },

  async onGenerate() {
    if (!this.data.sessionId || this.data.generating) return
    if (this.data.userTurnCount < 1) {
      this.setData({ error: '请先回答几个问题，再生成回忆录' })
      return
    }
    this.setData({
      generating: true,
      error: '',
      waitHint: '正在把口述整理成章节…',
    })
    wx.showLoading({ title: '生成中…', mask: true })
    try {
      const generated = await generateChapter(this.data.sessionId)
      this.onGenerated(generated)
      wx.showToast({ title: '章节已生成', icon: 'success' })
    } catch (e: any) {
      this.setData({ error: (e && e.message) || '生成失败' })
    } finally {
      wx.hideLoading()
      this.setData({ generating: false, waitHint: '' })
    }
  },

  onGenerated(generated: GeneratedChapter) {
    this._lastGenerated = generated
    this.setData({
      generatedPreview: true,
      generationStatus: 'ready',
      saveHint:
        generated.trigger === 'auto'
          ? `已满${generated.user_turn_count}轮，章节已生成`
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
    if ((g && g.chapter && g.chapter.id)) {
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
    if (!this.data.sessionId || this.data.sending) return
    this.setData({ sending: true, error: '', waitHint: '正在结束…' })
    try {
      await postMessage(this.data.sessionId, '结束本次采访', 'end')
      await finishInterview(this.data.sessionId)
      this.setData({ finished: true, saveHint: '采访已结束，记录已保存' })
      await this.reloadMessages()
    } catch (e: any) {
      this.setData({ error: (e && e.message) || '结束失败' })
    } finally {
      this.setData({ sending: false, waitHint: '' })
    }
  },
})
