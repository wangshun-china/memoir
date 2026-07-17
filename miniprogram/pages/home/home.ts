import {
  ensureLogin,
  isLoggedIn,
  listMemoirs,
  Memoir,
  wechatLogin,
} from '../../services/api'

Page({
  data: {
    memoirs: [] as Memoir[],
    loading: true,
    error: '',
    loggedIn: false,
    loggingIn: false,
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
      const memoirs = await listMemoirs()
      this.setData({ memoirs, loading: false, loggedIn: true })
    } catch (e: any) {
      this.setData({
        loading: false,
        error: e?.message || '加载失败',
        loggedIn: isLoggedIn(),
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
        error: e?.message || '微信登录失败',
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

  onOpenMemoir(e: WechatMiniprogram.TouchEvent) {
    const id = e.currentTarget.dataset.id as string
    wx.navigateTo({
      url: `/pages/interview/interview?memoirId=${id}&topic=${encodeURIComponent('童年与家庭')}`,
    })
  },

  onStartInterview(e: WechatMiniprogram.TouchEvent) {
    const id = e.currentTarget.dataset.id as string
    wx.navigateTo({
      url: `/pages/interview/interview?memoirId=${id}&topic=${encodeURIComponent('童年与家庭')}`,
    })
  },
})
