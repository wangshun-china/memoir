import {
  adminAiConfig,
  adminAiUsage,
  adminBindWechat,
  adminMemoirs,
  adminOverview,
  adminPutAiConfig,
  adminTestAi,
  adminUsers,
  getMe,
  isLoggedIn,
  AdminMemoirRow,
  AdminOverview,
  AdminUserRow,
} from '../../services/api'

type Tab = 'overview' | 'users' | 'memoirs' | 'ai'

Page({
  data: {
    tab: 'overview' as Tab,
    loading: true,
    error: '',
    overview: null as AdminOverview | null,
    users: [] as AdminUserRow[],
    memoirs: [] as AdminMemoirRow[],
    aiBase: '',
    aiModel: '',
    aiKey: '',
    aiKeySet: false,
    aiKeyMasked: '',
    aiMode: '',
    aiThinking: true,
    testPrompt: '请用一句话自我介绍你是回忆录采访助手。',
    testResult: '',
    testing: false,
    savingAi: false,
    wechatBound: false,
    wechatMasked: '',
    bindingWechat: false,
    usageSummary: '',
    usageLines: [] as string[],
  },

  onShow() {
    this.bootstrap()
  },

  async bootstrap() {
    if (!isLoggedIn()) {
      wx.showToast({ title: '请先登录', icon: 'none' })
      setTimeout(() => wx.switchTab({ url: '/pages/mine/mine' }), 400)
      return
    }
    try {
      const me = await getMe()
      if (!me.is_admin) {
        wx.showModal({
          title: '无权限',
          content: '当前账号不是管理员',
          showCancel: false,
          success: () => wx.navigateBack({ fail: () => wx.switchTab({ url: '/pages/mine/mine' }) }),
        })
        return
      }
      this.loadWechatBindState()
      await this.loadTab(this.data.tab)
    } catch (e: any) {
      this.setData({ loading: false, error: (e && e.message) || '加载失败' })
    }
  },

  loadWechatBindState() {
    const saved = wx.getStorageSync('memoir_admin_wechat') as { bound?: boolean; openid_masked?: string } | ''
    if (saved && saved.bound) {
      this.setData({ wechatBound: true, wechatMasked: saved.openid_masked || '' })
    }
  },

  async onBindWechat() {
    if (this.data.bindingWechat) return
    this.setData({ bindingWechat: true, error: '' })
    try {
      const code = await new Promise<string>((resolve, reject) => {
        wx.login({
          success: (res) => {
            if (res.code) resolve(res.code)
            else reject(new Error('未获取到登录 code'))
          },
          fail: (err) => reject(new Error(err.errMsg || 'wx.login 调用失败')),
        })
      })
      const r = await adminBindWechat(code)
      wx.setStorageSync('memoir_admin_wechat', { bound: true, openid_masked: r.openid_masked })
      this.setData({ bindingWechat: false, wechatBound: true, wechatMasked: r.openid_masked })
      wx.showModal({
        title: '绑定成功',
        content: r.promoted
          ? '该微信账号已提升为管理员。退出后点「微信一键登录」即可进入管理台。'
          : '已绑定。退出后点「微信一键登录」即可进入管理台，不再依赖账号密码。',
        showCancel: false,
      })
    } catch (e: any) {
      this.setData({
        bindingWechat: false,
        error: (e && e.message) || '绑定失败',
      })
    }
  },

  async loadTab(tab: Tab) {
    this.setData({ tab, loading: true, error: '' })
    try {
      if (tab === 'overview') {
        const overview = await adminOverview()
        this.setData({ overview, loading: false })
      } else if (tab === 'users') {
        const users = await adminUsers()
        this.setData({ users: users || [], loading: false })
      } else if (tab === 'memoirs') {
        const memoirs = await adminMemoirs()
        this.setData({ memoirs: memoirs || [], loading: false })
      } else {
        const ai = await adminAiConfig()
        const usage = await adminAiUsage(15)
        const recent = (usage.recent || []).map((r) => {
          const ok = r.success ? 'OK' : 'FAIL'
          return ok + ' · ' + r.source + ' · ' + r.model + ' · ' + r.total_tokens + 'tok · ' + r.latency_ms + 'ms'
        })
        const s = usage.summary
        const usageSummary =
          '调用 ' +
          s.calls +
          ' · 成功 ' +
          s.success_calls +
          ' · tokens ' +
          s.total_tokens +
          ' · 均时 ' +
          Math.round(s.avg_latency_ms || 0) +
          'ms'
        this.setData({
          aiBase: ai.api_base || '',
          aiModel: ai.model || '',
          aiKeySet: !!ai.api_key_set,
          aiKeyMasked: ai.api_key_masked || '',
          aiMode: ai.mode || '',
          aiThinking: ai.enable_thinking !== false,
          aiKey: '',
          usageSummary,
          usageLines: recent,
          loading: false,
        })
      }
    } catch (e: any) {
      this.setData({ loading: false, error: (e && e.message) || '加载失败' })
    }
  },

  onTab(e: WechatMiniprogram.TouchEvent) {
    const tab = e.currentTarget.dataset.tab as Tab
    if (!tab || tab === this.data.tab) return
    this.loadTab(tab)
  },

  onAiBase(e: WechatMiniprogram.Input) {
    this.setData({ aiBase: e.detail.value || '' })
  },
  onAiModel(e: WechatMiniprogram.Input) {
    this.setData({ aiModel: e.detail.value || '' })
  },
  onAiKey(e: WechatMiniprogram.Input) {
    this.setData({ aiKey: e.detail.value || '' })
  },
  onAiThinking(e: WechatMiniprogram.SwitchChange) {
    this.setData({ aiThinking: !!e.detail.value })
  },
  onTestPrompt(e: WechatMiniprogram.Input) {
    this.setData({ testPrompt: e.detail.value || '' })
  },

  async onSaveAi() {
    this.setData({ savingAi: true, error: '' })
    try {
      const data: { api_base?: string; model?: string; api_key?: string; enable_thinking?: boolean } = {
        api_base: (this.data.aiBase || '').trim(),
        model: (this.data.aiModel || '').trim(),
        enable_thinking: !!this.data.aiThinking,
      }
      const key = (this.data.aiKey || '').trim()
      if (key) data.api_key = key
      const ai = await adminPutAiConfig(data)
      this.setData({
        savingAi: false,
        aiKey: '',
        aiKeySet: !!ai.api_key_set,
        aiKeyMasked: ai.api_key_masked || '',
        aiMode: ai.mode || '',
        aiThinking: ai.enable_thinking !== false,
      })
      wx.showToast({ title: '已保存', icon: 'success' })
    } catch (e: any) {
      this.setData({ savingAi: false, error: (e && e.message) || '保存失败' })
    }
  },

  async onTestAi() {
    this.setData({ testing: true, testResult: '', error: '' })
    try {
      const r = await adminTestAi(this.data.testPrompt)
      const text = r.ok
        ? '✓ ' + (r.reply || '') + '\n(' + r.model + ' · ' + r.latency_ms + 'ms)'
        : '✗ ' + (r.error || '失败')
      this.setData({ testing: false, testResult: text })
    } catch (e: any) {
      this.setData({ testing: false, testResult: '✗ ' + ((e && e.message) || '请求失败') })
    }
  },
})
