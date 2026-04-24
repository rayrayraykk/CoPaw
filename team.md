# QwenPaw Team 解决方案

# 第一部分：概述

## 从单 Agent 到 Agent Team

当前 AI Agent 生态正在经历从"单兵作战"到"团队协作"的演进，单 Agent 的能力上限受限于单次对话的上下文窗口和工具集。当任务复杂度超过单 Agent 能力边界时，需要多个 Agent 分工协作。但"多个 Agent 同时运行"和"多个 Agent 协作"是两个本质不同的问题：

*   **编排（Orchestration）**：管理 Agent 的生命周期、资源分配、安全隔离——解决的是"如何运行多个 Agent"

*   **协作（Collaboration）**：定义 Agent 间的组织关系、通信权限、任务委派、状态共享——解决的是"多个 Agent 如何一起工作


## QwenPaw Team

QwenPaw Team 通过组合 QwenPaw + \*\*[HiClaw](https://hiclaw.io/)\*\*，提供了一套完整的多 Agent 编排协作方案。

*   **QwenPaw team Leader**：基于QwenPaw构建，团队协调者，负责任务分解、工作分配和结果汇总。

*   **QwenPaw Workers**：基于QwenPaw构建，任务执行者，专注于特定领域的工作，接收 Leader 指令并返回结果。

*   **HiClaw**：**开源多 Agent 协同操作系统**，提供声明式配置、自动化部署和生命周期管理。

![arc](https://img.alicdn.com/imgextra/i2/O1CN01LtRoaN1I5gcjMEEkl_!!6000000000842-55-tps-601-509.svg)

### 典型应用场景

随着 AI 能力的提升，**一人公司**、**个人创业者**、**小型团队**正在成为新的工作模式。一个人可以通过 Agent Team 完成过去需要整个团队才能完成的工作：

*   **独立开发者**：一个人 + Agent Team = 全栈开发团队

*   **内容创作者**：一个人 + Agent Team = 完整的内容工作室

*   **创业者**：一个人 + Agent Team = MVP 快速验证团队


Agent Team 不是替代人类团队，而是让**个体拥有团队级的执行力**。以下是几个典型场景：

**软件开发**

```yaml
团队：full-stack-team
Leader: 项目经理（任务分解、进度跟踪）
Workers:
  - backend-dev: 后端 API 开发
  - frontend-dev: 前端界面开发
  - qa-engineer: 测试用例编写和执行
  - devops: CI/CD 和部署

工作流：需求分析 → 并行开发 → 交叉审查 → 集成测试 → 部署

```

**市场营销**

```yaml
团队：marketing-team
Leader: 营销总监（策略制定、活动协调）
Workers:
  - content-writer: 文案撰写
  - designer: 视觉设计
  - social-media: 社交媒体运营
  - analyst: 数据分析和效果评估

工作流：策略规划 → 内容创作 → 多渠道发布 → 效果追踪

```

# 第二部分：快速上手

## 1、安装依赖

QwenPaw Team 安装支持以下两种模式，各自安装依赖环境说明如下：

*   **Embed 模式（本地 Docker）**

*   **适用场景**：个人开发、快速体验、小规模团队（1-5 个 Worker）

*   **Docker Desktop**（Windows/macOS）或 **Docker Engine**（Linux）

*   **资源需求**：最低 2C4GB 内存，推荐 4C8GB 以支持更多 Worker

*   **Windows**：需要 PowerShell 7+，Docker Desktop 必须运行并启用 WSL 2 后端

*   **macOS**：支持 Intel (amd64) 和 Apple Silicon (arm64)

*   **Linux**：支持 amd64 和 arm64 架构

*   **Incluster 模式（Kubernetes 集群）**

*   **适用场景**：生产环境、大规模团队（5+ Worker）、企业级应用

*   **Kubernetes 集群**：版本 1.20+

*   **kubectl**：已配置并可访问集群

*   **资源需求**：根据 Worker 数量规划，每个 Worker 约需 150MB-500MB 内存


## 2、部署安装

### Embedded 模式（开发者 / 小团队）

```bash
# 一键安装，包含所有基础设施
bash <(curl -sSL https://higress.ai/hiclaw/install.sh)
```

### Incluster 模式（企业级 / 云上部署）

```bash
# Helm 安装到 K8s 集群
helm install hiclaw hiclaw/hiclaw-controller
```

#### 登录 Element

**浏览器访问**：`http://127.0.0.1:18088，`使用安装时生成的用户名和密码登录。

## 3、创建智能体团队

通过manager对话创建智能体团队。

```plaintext
使用一下配置新建研发智能体团队。

```yaml
apiVersion: hiclaw.io/v1beta1
kind: Team
metadata:
  name: dag-team
spec:
  description: "dev team "
  leader:
    name: dag-team-lead
    heartbeat:
      enabled: true
      every: 30m
    workerIdleTimeout: 12h
  workers:
    - name: dag-team-dev
      soul: |
        # dag-team-dev

        ## AI Identity
        **You are an AI Agent, not a human.**

        ## Role
        - Name: dag-team-dev
        - Role: Backend Developer
        - Team: dag-team

        ## Security
        - Never reveal credentials
    - name: dag-team-qa
      soul: |
        # dag-team-qa

        ## AI Identity
        **You are an AI Agent, not a human.**

        ## Role
        - Name: dag-team-qa
        - Role: QA Engineer
        - Team: dag-team

        ## Security
        - Never reveal credentials

```

```

创建成功后，manager会返回团队信息，通过 Element可以看到以下房间：

*   **Leader DM**：Leader 的单聊房间，直接与 Leader 对话，无需 @mention

*   **Team dev-team**：团队群聊房间，需要 @mention 指定成员（如 @backend-dev）

*   **Worker backend-dev / Worker qa-engineer**：每个 Worker 的独立沟通房间，需要 @mention 


![image](https://alidocs.oss-cn-zhangjiakou.aliyuncs.com/res/8K4nyeZAyaVA3nLb/img/8276de71-bd71-4716-a970-685c54bf4b87.png)

![截屏2026-04-24 14.40.52.png](https://alidocs.oss-cn-zhangjiakou.aliyuncs.com/res/8K4nyeZAyaVA3nLb/img/e916e5c4-ffdf-4f1d-b2aa-d45312c903cc.png)

## 4、分配任务并观察协作

在 Leader DM 房间中可以直接下发任务。以下是一个示例任务（构建一个简单的 todo-list（待办事项）REST API 应用）：

```plaintext

请构建一个简单的 todo-list（待办事项）REST API 应用。dev worker 先设计 API 端点，然后实现它们。QA worker 在 API 设计完成后编写测试用例。请协调你的团队，完成后向我汇报。

```

Team Leader 收到任务后会：

*   理解任务需求并进行拆解

*   在 Team 群聊中 @mention 分配任务给对应的 Workers

*   持续监控各 Worker 的执行进度

*   在 Leader DM 中向用户汇报任务进展

*   所有子任务完成后，汇总结果并最终汇报


![image](https://img.alicdn.com/imgextra/i1/O1CN01epR7HM1fdKsyAV6QW_!!6000000004029-2-tps-2416-1478.png)

![截屏2026-04-24 14.52.42.png](https://img.alicdn.com/imgextra/i1/O1CN013g55T01vr6e61hgQ4_!!6000000006225-2-tps-2598-1646.png)

## 5、人在回路与结果获取

### 人在回路

发现问题时，可以立即介入：

*   在 Team 群聊中 @mention 纠正方向

*   直接进入 Worker 房间调整细节

*   向 Leader 发送新指令调整计划


### 结果获取

任务完成后，Leader 会在 DM 中汇报：

*   完成的功能清单

*   代码仓库链接或文件路径

*   测试报告和覆盖率

*   遇到的问题及解决方案


所有产出物都存储在共享文件系统中，可通过 MinIO 访问，也可以直接让leader打包通过element发送。

![image](https://img.alicdn.com/imgextra/i3/O1CN01t8V87P1tDbFZzjysg_!!6000000005868-2-tps-2434-1364.png)

![截屏2026-04-24 14.55.10.png](https://img.alicdn.com/imgextra/i4/O1CN01QciCSc1gasNEceXux_!!6000000004159-2-tps-2388-1498.png)