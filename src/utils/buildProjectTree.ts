import type { SessionSummary } from "../types";

/** 项目树节点 */
export interface ProjectNode {
  /** 完整路径（未分类节点为空字符串） */
  path: string;
  /** 显示名称（最后一段目录名） */
  name: string;
  /** 该路径下的 sessions（不含子路径的） */
  sessions: SessionSummary[];
  /** 子路径节点 */
  children: ProjectNode[];
  /** 该节点及所有子节点的 session 总数 */
  totalCount: number;
}

/** 将 session 时间统一转成可比较的时间戳，非法值按 0 处理 */
function getStartedAtTime(session: SessionSummary): number {
  if (!session.started_at) {
    return 0;
  }

  const timestamp = Date.parse(session.started_at);
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

/** 按开始时间倒序排列，保证目录视图和普通列表保持同一时间语义 */
function sortSessionsByStartedAtDesc(sessions: SessionSummary[]): SessionSummary[] {
  return [...sessions].sort(
    (a, b) => getStartedAtTime(b) - getStartedAtTime(a)
  );
}

/**
 * 将扁平的 session 列表按 project_path 构建为树结构
 *
 * 实现逻辑：
 * 1. 按 project_path 分组
 * 2. 检测父子路径关系，构建层级树
 * 3. 无路径的归入 "ungrouped" 节点
 */
export function buildProjectTree(
  sessions: SessionSummary[],
  ungroupedLabel: string
): ProjectNode[] {
  // 按 project_path 分组
  const grouped = new Map<string, SessionSummary[]>();
  const ungrouped: SessionSummary[] = [];

  for (const s of sessions) {
    if (s.project_path) {
      const existing = grouped.get(s.project_path);
      if (existing) {
        existing.push(s);
      } else {
        grouped.set(s.project_path, [s]);
      }
    } else {
      ungrouped.push(s);
    }
  }

  // 排序路径，短路径在前（父路径先出现）
  const paths = [...grouped.keys()].sort();

  // 构建树：检测父子路径关系
  const roots: ProjectNode[] = [];
  const nodeMap = new Map<string, ProjectNode>();
  const latestStartedAtMap = new Map<string, number>();

  for (const path of paths) {
    const node: ProjectNode = {
      path,
      name: path.split("/").filter(Boolean).pop() || path,
      sessions: sortSessionsByStartedAtDesc(grouped.get(path) || []),
      children: [],
      totalCount: 0,
    };
    nodeMap.set(path, node);

    // 查找最近的父路径
    let parentFound = false;
    let candidate = path;
    while (candidate.includes("/")) {
      candidate = candidate.substring(0, candidate.lastIndexOf("/"));
      if (candidate && nodeMap.has(candidate)) {
        nodeMap.get(candidate)!.children.push(node);
        parentFound = true;
        break;
      }
    }

    if (!parentFound) {
      roots.push(node);
    }
  }

  function compareNodesByLatestActivity(a: ProjectNode, b: ProjectNode): number {
    const latestDiff =
      (latestStartedAtMap.get(b.path) || 0) -
      (latestStartedAtMap.get(a.path) || 0);
    if (latestDiff !== 0) {
      return latestDiff;
    }

    const countDiff = b.totalCount - a.totalCount;
    if (countDiff !== 0) {
      return countDiff;
    }

    return a.path.localeCompare(b.path);
  }

  // 自底向上计算聚合信息，并让每层目录都按最近活跃时间倒序排列
  function calcNodeMeta(node: ProjectNode): number {
    const ownLatestStartedAt = node.sessions[0]
      ? getStartedAtTime(node.sessions[0])
      : 0;

    let latestStartedAt = ownLatestStartedAt;
    let totalCount = node.sessions.length;

    for (const child of node.children) {
      const childLatestStartedAt = calcNodeMeta(child);
      latestStartedAt = Math.max(latestStartedAt, childLatestStartedAt);
      totalCount += child.totalCount;
    }

    node.totalCount = totalCount;
    latestStartedAtMap.set(node.path, latestStartedAt);
    node.children.sort(compareNodesByLatestActivity);

    return latestStartedAt;
  }

  roots.forEach(calcNodeMeta);

  // 根目录也按最近活跃时间倒序排列
  roots.sort(compareNodesByLatestActivity);

  // 未分类节点放最后
  if (ungrouped.length > 0) {
    roots.push({
      path: "",
      name: ungroupedLabel,
      sessions: sortSessionsByStartedAtDesc(ungrouped),
      children: [],
      totalCount: ungrouped.length,
    });
  }

  return roots;
}
