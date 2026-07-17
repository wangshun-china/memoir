// app.ts — no mock login; pages call ensureLogin / wechatLogin when needed.

App<IAppOption>({
  globalData: {
    userInfo: null as WechatMiniprogram.UserInfo | null,
  },
  onLaunch() {
    // Session restore happens per-page via ensureLogin (validates token with /me).
  },
})
