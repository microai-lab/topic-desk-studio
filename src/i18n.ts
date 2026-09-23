/** Topic Desk Studio — minimal i18n, zero external dependencies. */

export type Locale = 'zh' | 'en'
export type Theme  = 'light' | 'dark' | 'system'

/** All translatable strings for one locale. */
export interface Messages {
  // Nav
  navDiscover: string
  navQueue: string
  navNew: string
  navSettings: string
  // Topbar
  btnRefresh: string
  btnRefreshing: string
  btnQuery: string
  btnQuerying: string
  // Filters
  filterSource: string
  filterAllSources: string
  filterRegion: string
  filterAllRegions: string
  filterDomestic: string
  filterInternational: string
  filterCategory: string
  filterAllCategories: string
  filterGeneral: string
  filterTech: string
  filterFinance: string
  filterDev: string
  filterSearch: string
  filterSearchPlaceholder: string
  filterSort: string
  filterSortRank: string
  filterSortUpdated: string
  // Results
  resultsTotal: (n: number) => string
  resultsErrors: (n: number) => string
  resultsLoading: string
  resultsEmptyTitle: string
  resultsEmptyBody: string
  // Topic card
  rankLabel: string
  trendAccum: string
  translateBtn: string
  translating: string
  translated: string
  firstSeen: string
  consecutive: string
  consecutiveUnit: string
  queueAdd: string
  queueRemove: string
  saving: string
  // Pagination
  prev: string
  next: string
  // Settings
  settingsTitle: string
  settingsBack: string
  tabGeneral: string
  tabSources: string
  tabModel: string
  tabStorage: string
  settingsGeneralDesc: string
  settingsSourcesDesc: string
  settingsNetworkDesc: string
  // General tab — appearance
  sectionAppearance: string
  labelTheme: string
  themeLight: string
  themeDark: string
  themeSystem: string
  // General tab — language
  sectionLanguage: string
  langZh: string
  langEn: string
  sectionNetwork: string
  labelProxy: string
  proxyPlaceholder: string
  proxyHelp: string
  networkSaveNotice: string
  storageHeading: string
  storageDesc: string
  storageHealthy: string
  storageDamaged: string
  storageTopics: string
  storageTrends: string
  storageRuns: string
  storageBrowser: string
  storageTopicDb: string
  storageBrowserDb: string
  storageLatestBackup: string
  storageNoBackup: string
  storageOpenFolder: string
  storageOptimize: string
  storageBackup: string
  storageRestore: string
  storageWorking: string
  storageRestoreConfirm: string
  // Sources tab
  regionDomestic: string
  regionIntl: string
  notCollected: string
  topicCount: (n: number) => string
  xhsLoginCollect: string
  xhsCollectPage: string
  sourceAdd: string
  sourceEdit: string
  sourceCustom: string
  sourceBuiltIn: string
  sourceCode: string
  sourceName: string
  sourceHome: string
  sourceEndpoint: string
  sourceParser: string
  sourceProxyMode: string
  sourceAuto: string
  sourceDirect: string
  sourceProxy: string
  sourceItemsPath: string
  sourceTitlePath: string
  sourceUrlPath: string
  sourceIdPath: string
  sourcePublishedPath: string
  sourceRankPath: string
  sourceHeatPath: string
  sourceItemSelector: string
  sourceTitleSelector: string
  sourceLinkSelector: string
  sourceCancel: string
  sourceSaved: string
  sourceImport: string
  sourceExport: string
  sourceImportAll: string
  sourceExportAll: string
  sourceRestoreDefaults: string
  sourceImported: string
  sourceExported: string
  sourceDefaultsRestored: string
  sourceDeleteConfirm: (name: string) => string
  sourceRestoreConfirm: string
  // Model tab
  modelHeading: string
  modelDesc: string
  labelEndpoint: string
  labelModel: string
  labelApiKey: string
  apiKeyPlaceholder: string
  apiKeySavedPlaceholder: string
  btnSave: string
  btnSaving: string
  saveNotice: string
}

const zh: Messages = {
  navDiscover: '发现选题',
  navQueue: '待创作',
  navNew: '最近新增',
  navSettings: '设置',
  btnRefresh: '采集数据',
  btnRefreshing: '采集中…',
  btnQuery: '刷新列表',
  btnQuerying: '查询中…',
  filterSource: '来源',
  filterAllSources: '全部来源',
  filterRegion: '地区',
  filterAllRegions: '全部地区',
  filterDomestic: '国内',
  filterInternational: '国外',
  filterCategory: '分类',
  filterAllCategories: '全部分类',
  filterGeneral: '综合',
  filterTech: '科技与 AI',
  filterFinance: '财经市场',
  filterDev: '开发者',
  filterSearch: '搜索',
  filterSearchPlaceholder: '输入标题关键词…',
  filterSort: '排序',
  filterSortRank: '榜单排名',
  filterSortUpdated: '最近更新',
  resultsTotal: (n) => `${n} 个选题`,
  resultsErrors: (n) => `${n} 个来源异常`,
  resultsLoading: '正在读取本地选题库…',
  resultsEmptyTitle: '本地选题库还是空的',
  resultsEmptyBody: '点击“采集数据”开始采集，之后可以在这里筛选、追踪并加入待创作。',
  rankLabel: 'RANK',
  trendAccum: '趋势积累中',
  translateBtn: '译为中文',
  translating: '翻译中…',
  translated: '已翻译',
  firstSeen: '首次发现',
  consecutive: '连续上榜',
  consecutiveUnit: '轮',
  queueAdd: '加入待创作',
  queueRemove: '移出待创作',
  saving: '保存中…',
  prev: '上一页',
  next: '下一页',
  settingsTitle: '设置',
  settingsBack: '返回',
  tabGeneral: '通用',
  tabSources: '数据来源',
  tabModel: '翻译模型',
  tabStorage: '本地数据',
  settingsGeneralDesc: '调整界面外观与语言偏好，设置会自动保存在本机。',
  settingsSourcesDesc: '管理采集来源、解析方式和单独的网络路由。',
  settingsNetworkDesc: '设置采集服务使用的本地代理地址和连接策略。',
  sectionAppearance: '外观',
  labelTheme: '主题',
  themeLight: '浅色',
  themeDark: '深色',
  themeSystem: '随系统',
  sectionLanguage: '语言',
  langZh: '中文',
  langEn: 'English',
  sectionNetwork: '网络代理',
  labelProxy: 'HTTP(S) 代理',
  proxyPlaceholder: 'http://127.0.0.1:7897',
  proxyHelp: '仅直连受限的境外来源使用该代理；国内及可直连来源保持直连。留空表示全部直连。',
  networkSaveNotice: '采集代理设置已保存。',
  storageHeading: '本地数据管理',
  storageDesc: '话题、采集历史和浏览记录均保存在本机。系统会自动汇总趋势并清理过期运行记录。',
  storageHealthy: '数据库状态正常',
  storageDamaged: '数据库完整性异常',
  storageTopics: '话题',
  storageTrends: '趋势记录',
  storageRuns: '采集运行',
  storageBrowser: '浏览记录',
  storageTopicDb: '选题数据库',
  storageBrowserDb: '浏览数据库',
  storageLatestBackup: '最近备份',
  storageNoBackup: '尚未创建',
  storageOpenFolder: '打开数据目录',
  storageOptimize: '立即整理',
  storageBackup: '创建备份',
  storageRestore: '恢复最近备份',
  storageWorking: '处理中…',
  storageRestoreConfirm: '恢复将覆盖当前本地数据，并保留下载文件。确定继续吗？',
  regionDomestic: '国内',
  regionIntl: '国际',
  notCollected: '尚未采集',
  topicCount: (n) => `${n} 条话题`,
  xhsLoginCollect: '登录采集',
  xhsCollectPage: '采集当前页',
  sourceAdd: '添加数据源',
  sourceEdit: '编辑数据源',
  sourceCustom: '自定义',
  sourceBuiltIn: '内置',
  sourceCode: '来源代码',
  sourceName: '显示名称',
  sourceHome: '网站首页',
  sourceEndpoint: '采集地址',
  sourceParser: '解析方式',
  sourceProxyMode: '网络方式',
  sourceAuto: '自动判断',
  sourceDirect: '直连',
  sourceProxy: '使用代理',
  sourceItemsPath: '列表路径',
  sourceTitlePath: '标题字段',
  sourceUrlPath: '链接字段',
  sourceIdPath: '稳定 ID 字段（可选）',
  sourcePublishedPath: '发布时间字段（可选）',
  sourceRankPath: '排名字段（可选）',
  sourceHeatPath: '热度字段（可选）',
  sourceItemSelector: '条目 CSS 选择器',
  sourceTitleSelector: '标题 CSS 选择器',
  sourceLinkSelector: '链接 CSS 选择器（默认同标题）',
  sourceCancel: '取消',
  sourceSaved: '数据源配置已保存。',
  sourceImport: '导入',
  sourceExport: '导出',
  sourceImportAll: '批量导入',
  sourceExportAll: '批量导出',
  sourceRestoreDefaults: '恢复默认来源',
  sourceImported: '数据源导入完成。',
  sourceExported: '数据源文件已导出。',
  sourceDefaultsRestored: '默认数据源已恢复。',
  sourceDeleteConfirm: (name) => `删除数据源“${name}”？历史话题仍会保留。`,
  sourceRestoreConfirm: '恢复全部默认数据源？已修改的默认来源会重置，但自定义来源和历史话题不会删除。',
  modelHeading: '英文标题翻译',
  modelDesc: '接口地址与模型名保存在本地 SQLite；API Key 加密后入库，仅在调用模型时于内存解密。',
  labelEndpoint: '接口地址',
  labelModel: '模型名称',
  labelApiKey: 'API Key',
  apiKeyPlaceholder: '输入后加密保存到本地 SQLite',
  apiKeySavedPlaceholder: '已保存；留空表示不修改',
  btnSave: '保存设置',
  btnSaving: '保存中…',
  saveNotice: '模型设置已保存到本地 SQLite。',
}

const en: Messages = {
  navDiscover: 'Discover',
  navQueue: 'Queue',
  navNew: 'Recent additions',
  navSettings: 'Settings',
  btnRefresh: 'Collect',
  btnRefreshing: 'Collecting…',
  btnQuery: 'Refresh list',
  btnQuerying: 'Querying…',
  filterSource: 'Source',
  filterAllSources: 'All sources',
  filterRegion: 'Region',
  filterAllRegions: 'All regions',
  filterDomestic: 'China',
  filterInternational: 'International',
  filterCategory: 'Category',
  filterAllCategories: 'All categories',
  filterGeneral: 'General',
  filterTech: 'Tech & AI',
  filterFinance: 'Finance',
  filterDev: 'Developer',
  filterSearch: 'Search',
  filterSearchPlaceholder: 'Search by keyword…',
  filterSort: 'Sort',
  filterSortRank: 'By rank',
  filterSortUpdated: 'Latest',
  resultsTotal: (n) => `${n} topic${n !== 1 ? 's' : ''}`,
  resultsErrors: (n) => `${n} source error${n !== 1 ? 's' : ''}`,
  resultsLoading: 'Loading topics…',
  resultsEmptyTitle: 'Nothing here yet',
  resultsEmptyBody: 'Click Collect to start collecting. Topics will appear here for you to filter, track, and queue.',
  rankLabel: 'RANK',
  trendAccum: 'Building trend',
  translateBtn: 'Translate',
  translating: 'Translating…',
  translated: 'Translated',
  firstSeen: 'First seen',
  consecutive: 'On chart',
  consecutiveUnit: 'runs',
  queueAdd: 'Add to queue',
  queueRemove: 'Remove',
  saving: 'Saving…',
  prev: 'Previous',
  next: 'Next',
  settingsTitle: 'Settings',
  settingsBack: 'Back',
  tabGeneral: 'General',
  tabSources: 'Sources',
  tabModel: 'Translation',
  tabStorage: 'Local data',
  settingsGeneralDesc: 'Adjust appearance and language preferences. Changes are saved locally.',
  settingsSourcesDesc: 'Manage collection sources, parsers, and per-source network routing.',
  settingsNetworkDesc: 'Configure the local proxy address and connection policy used for collection.',
  sectionAppearance: 'Appearance',
  labelTheme: 'Theme',
  themeLight: 'Light',
  themeDark: 'Dark',
  themeSystem: 'System',
  sectionLanguage: 'Language',
  langZh: '中文',
  langEn: 'English',
  sectionNetwork: 'Network proxy',
  labelProxy: 'HTTP(S) proxy',
  proxyPlaceholder: 'http://127.0.0.1:7897',
  proxyHelp: 'Only restricted international sources use this proxy. Domestic and directly reachable sources remain direct. Leave blank to make every source direct.',
  networkSaveNotice: 'Collection proxy saved.',
  storageHeading: 'Local Data',
  storageDesc: 'Topics, collection history, and browser records stay on this device. Trends are rolled up and expired run data is pruned automatically.',
  storageHealthy: 'Databases healthy',
  storageDamaged: 'Database integrity issue',
  storageTopics: 'Topics',
  storageTrends: 'Trend records',
  storageRuns: 'Collection runs',
  storageBrowser: 'Browser records',
  storageTopicDb: 'Topic database',
  storageBrowserDb: 'Browser database',
  storageLatestBackup: 'Latest backup',
  storageNoBackup: 'Not created yet',
  storageOpenFolder: 'Open data folder',
  storageOptimize: 'Optimize now',
  storageBackup: 'Create backup',
  storageRestore: 'Restore latest',
  storageWorking: 'Working…',
  storageRestoreConfirm: 'Restoring replaces current local data but keeps downloaded files. Continue?',
  regionDomestic: 'China',
  regionIntl: 'Intl',
  notCollected: 'Not yet collected',
  topicCount: (n) => `${n} topic${n !== 1 ? 's' : ''}`,
  xhsLoginCollect: 'Login & collect',
  xhsCollectPage: 'Collect page',
  sourceAdd: 'Add source',
  sourceEdit: 'Edit source',
  sourceCustom: 'Custom',
  sourceBuiltIn: 'Built-in',
  sourceCode: 'Source code',
  sourceName: 'Display name',
  sourceHome: 'Home page',
  sourceEndpoint: 'Collection URL',
  sourceParser: 'Parser',
  sourceProxyMode: 'Network route',
  sourceAuto: 'Automatic',
  sourceDirect: 'Direct',
  sourceProxy: 'Use proxy',
  sourceItemsPath: 'Items path',
  sourceTitlePath: 'Title field',
  sourceUrlPath: 'URL field',
  sourceIdPath: 'Stable ID field (optional)',
  sourcePublishedPath: 'Published field (optional)',
  sourceRankPath: 'Rank field (optional)',
  sourceHeatPath: 'Heat field (optional)',
  sourceItemSelector: 'Item CSS selector',
  sourceTitleSelector: 'Title CSS selector',
  sourceLinkSelector: 'Link CSS selector (defaults to title)',
  sourceCancel: 'Cancel',
  sourceSaved: 'Source configuration saved.',
  sourceImport: 'Import',
  sourceExport: 'Export',
  sourceImportAll: 'Import all',
  sourceExportAll: 'Export all',
  sourceRestoreDefaults: 'Restore defaults',
  sourceImported: 'Source import completed.',
  sourceExported: 'Source file exported.',
  sourceDefaultsRestored: 'Default sources restored.',
  sourceDeleteConfirm: (name) => `Delete source “${name}”? Historical topics will remain.`,
  sourceRestoreConfirm: 'Restore all default sources? Modified defaults will be reset, while custom sources and historical topics stay intact.',
  modelHeading: 'Title Translation',
  modelDesc: 'Endpoint and model name are stored in SQLite. The API key is encrypted at rest and decrypted in memory only for model calls.',
  labelEndpoint: 'Endpoint',
  labelModel: 'Model',
  labelApiKey: 'API Key',
  apiKeyPlaceholder: 'Encrypted in local SQLite',
  apiKeySavedPlaceholder: 'Saved — leave blank to keep',
  btnSave: 'Save settings',
  btnSaving: 'Saving…',
  saveNotice: 'Model settings saved to local SQLite.',
}

export const messages: Record<Locale, Messages> = { zh, en }

/** Choose the initial locale before Rust-backed preferences finish loading. */
export function detectLocale(): Locale {
  return navigator.language.startsWith('zh') ? 'zh' : 'en'
}

/** Use the operating-system appearance until Rust-backed preferences finish loading. */
export function detectTheme(): Theme {
  return 'system'
}

/** Apply a theme choice to the document root; 'system' removes the attribute. */
export function applyTheme(theme: Theme): void {
  const root = document.documentElement
  if (theme === 'system') {
    root.removeAttribute('data-theme')
  } else {
    root.dataset.theme = theme
  }
}
