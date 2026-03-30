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

  for (const path of paths) {
    const node: ProjectNode = {
      path,
      name: path.split("/").filter(Boolean).pop() || path,
      sessions: grouped.get(path) || [],
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

  // 计算每个节点的 totalCount（从叶到根）
  function calcTotal(node: ProjectNode): number {
    node.totalCount =
      node.sessions.length +
      node.children.reduce((sum, child) => sum + calcTotal(child), 0);
    return node.totalCount;
  }
  roots.forEach(calcTotal);

  // 按 session 总数降序排列
  roots.sort((a, b) => b.totalCount - a.totalCount);

  // 未分类节点放最后
  if (ungrouped.length > 0) {
    roots.push({
      path: "",
      name: ungroupedLabel,
      sessions: ungrouped,
      children: [],
      totalCount: ungrouped.length,
    });
  }

  return roots;
}
