// 与 src-tauri serde(rename_all = "camelCase") 结构一一对应

export interface MetricSample {
  tsMs: number;
  /** 总 CPU 使用率 0-100 */
  cpuTotal: number;
  /** 每核使用率 0-100 */
  cpuPerCore: number[];
  /** 平均主频 MHz */
  cpuFreqMhz: number;
  memUsed: number;
  memTotal: number;
  /** 提交内存(字节) */
  memCommitUsed: number;
  memCommitLimit: number;
  /** 硬缺页速率 页/秒 */
  pageInputPerSec: number;
  diskReadBps: number;
  diskWriteBps: number;
  /** 磁盘活跃占比 0-100 */
  diskActivePct: number;
  /** 平均单次传输耗时(秒) */
  diskAvgSecTransfer: number;
  diskQueueLength: number;
  diskTotal: number;
  diskFree: number;
  netRxBps: number;
  netTxBps: number;
  /** GPU 3D 引擎总占用 0-100,无计数器时 null */
  gpuUtilPct: number | null;
  /** GPU 专用显存 MB,无计数器时 null */
  gpuMemMb: number | null;
  /** DPC 速率 次/秒 */
  dpcPerSec: number;
  /** 中断时间占比 0-100 */
  interruptPct: number;
  temperatureC: number | null;
}

export interface ProcessInfo {
  pid: number;
  name: string;
  cpuPct: number;
  memMb: number;
  /** ⚠️ 含全部 I/O(含网络),非纯磁盘 */
  diskReadBps: number;
  diskWriteBps: number;
  exePath: string | null;
}

export interface DiskInfo {
  mount: string;
  total: number;
  free: number;
}

export interface LagReason {
  /** cpu | mem_pressure | page_faults | disk_latency | interrupt | hung_window | manual */
  kind: string;
  detail: string;
  value: number;
}

export interface TopProc {
  pid: number;
  name: string;
  value: number;
}

export interface GameSample {
  tsMs: number;
  app: string;
  pid: number;
  fps: number;
  avgGapMs: number;
  maxGapMs: number;
  stutters: number;
}

export interface LagEvent {
  id: number | null;
  tsMs: number;
  reasons: LagReason[];
  counters: MetricSample;
  topCpu: TopProc[];
  topMem: TopProc[];
  topDisk: TopProc[];
  hungWindows: string[];
  game: GameSample | null;
}
