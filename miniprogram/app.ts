// app.ts
import { ensureLogin } from './services/api'

App<IAppOption>({
  globalData: {},
  onLaunch() {
    // Stage-1: establish a mock session when possible; failures are non-fatal until pages load.
    ensureLogin().catch((err) => {
      console.warn('dev login deferred', err)
    })
  },
})
