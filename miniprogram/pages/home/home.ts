import {
  deleteMemoir,
  ensureLogin,
  isLoggedIn,
  listMemoirs,
  Memoir,
  passwordLogin,
  resetPassword,
  wechatLogin,
} from '../../services/api'

// eslint-disable-next-line @typescript-eslint/no-require-imports
const { normalizeMemoirList } = require('../../utils/memoir_status') as {
  normalizeMemoirList: (list: Memoir[]) => Memoir[]
}

Page({
  data: {
    memoirs: [] as Memoir[],
    loading: true,
    error: '',
    loggedIn: false,
    loggingIn: false,
    accountLoggingIn: false,
    accountUser: '',
    accountPass: '',
    showForgot: false,
    recoveryKey: '',
    newPass: '',
    resetting: false,
  },

  onShow() {
    this.load()
  },

  async load() {
    this.setData({ loading: true, error: '', loggedIn: isLoggedIn() })
    if (!isLoggedIn()) {
      this.setData({ loading: false, memoirs: [] })
      return
    }
    try {
      await ensureLogin()
      const raw = await listMemoirs()
      const memoirs = normalizeMemoirList(raw || [])
      this.setData({ memoirs, loading: false, loggedIn: true })
    } catch (e: any) {
      this.setData({
        loading: false,
        error: (e && e.message) || '加载失败',
        loggedIn: isLoggedIn(),
      })
    }
  },

  onAccountUser(e: WechatMiniprogram.Input) {
    this.setData({ accountUser: e.detail.value || '' })
  },

  onAccountPass(e: WechatMiniprogram.Input) {
    this.setData({ accountPass: e.detail.value || '' })
  },

  onRecoveryKey(e: WechatMiniprogram.Input) {
    this.setData({ recoveryKey: e.detail.value || '' })
  },

  onNewPass(e: WechatMiniprogram.Input) {
    this.setData({ newPass: e.detail.value || '' })
  },

  onToggleForgot() {
    this.setData({
      showForgot: !this.data.showForgot,
      error: '',
      recoveryKey: '',
      newPass: '',
    })
  },

  async onResetPassword() {
    const username = (this.data.accountUser || '').trim()
    const recoveryKey = this.data.recoveryKey || ''
    const newPass = this.data.newPass || ''
    if (!username || !recoveryKey || !newPass) {
      this.setData({ error: '请填写账号、恢复密钥和新密码' })
      return
    }
    this.setData({ resetting: true, error: '' })
    try {
      await resetPassword(username, recoveryKey, newPass)
      this.setData({
        resetting: false,
        showForgot: false,
        accountPass: newPass,
        recoveryKey: '',
        newPass: '',
      })
      wx.showToast({ title: '密码已重置', icon: 'success' })
    } catch (e: any) {
      this.setData({
        resetting: false,
        error: (e && e.message) || '重置失败',
      })
    }
  },

  async onAccountLogin() {
    const username = (this.data.accountUser || '').trim()
    const password = this.data.accountPass || ''
    if (!username || !password) {
      this.setData({ error: '请填写账号和密码' })
      return
    }
    this.setData({ accountLoggingIn: true, error: '' })
    try {
      const auth = await passwordLogin(username, password)
      this.setData({
        loggedIn: true,
        accountLoggingIn: false,
        accountPass: '',
      })
      if (auth.registered) {
        wx.showToast({ title: '已自动注册', icon: 'success' })
      } else if (auth.is_admin) {
        wx.showToast({ title: '管理员已登录', icon: 'none' })
      }
      await this.load()
    } catch (e: any) {
      this.setData({
        accountLoggingIn: false,
        error: (e && e.message) || '登录失败',
      })
    }
  },

  async onLogin() {
    this.setData({ loggingIn: true, error: '' })
    try {
      await wechatLogin()
      this.setData({ loggedIn: true, loggingIn: false })
      await this.load()
    } catch (e: any) {
      this.setData({
        loggingIn: false,
        error: (e && e.message) || '微信登录失败',
      })
    }
  },

  onCreate() {
    if (!isLoggedIn()) {
      wx.showToast({ title: '请先登录', icon: 'none' })
      return
    }
    wx.navigateTo({ url: '/pages/create/create' })
  },

  onOpenReader(e: WechatMiniprogram.TouchEvent) {
    const id = e.currentTarget.dataset.id as string
    if (!id) return
    wx.navigateTo({
      url: '/pages/reader/reader?memoirId=' + id,
    })
  },

  onStartInterview(e: WechatMiniprogram.TouchEvent) {
    const id = e.currentTarget.dataset.id as string
    wx.navigateTo({
      url:
        '/pages/interview/interview?memoirId=' +
        id +
        '&mode=start&topic=' +
        encodeURIComponent('童年与家庭'),
    })
  },

  onContinueInterview(e: WechatMiniprogram.TouchEvent) {
    const id = e.currentTarget.dataset.id as string
    const sessionId = (e.currentTarget.dataset.sessionId as string) || ''
    const topic = (e.currentTarget.dataset.topic as string) || '童年与家庭'
    if (!sessionId) {
      wx.navigateTo({
        url:
          '/pages/interview/interview?memoirId=' +
          id +
          '&mode=continue&topic=' +
          encodeURIComponent(topic),
      })
      return
    }
    wx.navigateTo({
      url:
        '/pages/interview/interview?memoirId=' +
        id +
        '&mode=continue&sessionId=' +
        sessionId +
        '&topic=' +
        encodeURIComponent(topic),
    })
  },

  /** 次要操作：查看正文 / 删除 — 避免误触 */
  onCardMore(e: WechatMiniprogram.TouchEvent) {
    const id = e.currentTarget.dataset.id as string
    const title = (e.currentTarget.dataset.title as string) || '这本回忆录'
    if (!id) return
    wx.showActionSheet({
      itemList: ['查看正文', '删除回忆录'],
      itemColor: '#2a2318',
      success: (res) => {
        if (res.tapIndex === 0) {
          wx.navigateTo({ url: '/pages/reader/reader?memoirId=' + id })
        } else if (res.tapIndex === 1) {
          this.confirmDelete(id, title)
        }
      },
    })
  },

  confirmDelete(id: string, title: string) {
    wx.showModal({
      title: '删除回忆录',
      content: '确定删除「' + title + '」？采访与草稿会一起删除，无法恢复。',
      confirmText: '删除',
      confirmColor: '#a33',
      success: async (res) => {
        if (!res.confirm) return
        try {
          await deleteMemoir(id)
          wx.showToast({ title: '已删除', icon: 'success' })
          await this.load()
        } catch (err: any) {
          wx.showToast({ title: (err && err.message) || '删除失败', icon: 'none' })
        }
      },
    })
  },
})
