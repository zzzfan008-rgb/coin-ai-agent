---
name: dreamina-cli
version: 1.0.0
description: 即梦（Dreamina）AI 生图/生视频工具，通过本地 dreamina CLI 调用
long_description: |
  即梦（Dreamina）是字节跳动旗下 AI 生成平台，支持文生图、图生图、文生视频、视频生视频。
  此 Skill 通过本机安装的 `dreamina` CLI 与平台通信，完成：
  - 登录态管理（OAuth Device Flow）
  - 积分查询
  - 文生图 / 图生图 / 图片放大
  - 文生视频 / 图生视频 / 首尾帧视频 / 多镜视频
  - 任务状态轮询与结果下载
  - 历史任务查询

  **图片引用格式**：用户上传的图片在消息中以 `[image: /uploads/chat-images/...]` 形式传给 LLM，
  image2image 命令使用 `--images` 参数引用图片路径（绝对路径，如 `/Users/lionfan/projects/fashion-ai-platform/uploads/chat-images/{org_id}/{uuid}.{ext}`）。
  注意：CLI 的 `--images` 需要真实文件路径，而非 URL。
permissions:
  - skill:dreamina-cli
  - tool:text2image
  - tool:image2image
  - tool:image_upscale
  - tool:text2video
  - tool:image2video
  - tool:frames2video
  - tool:multiframe2video
  - tool:multimodal2video
  - tool:user_credit
  - tool:list_task
  - tool:query_result
dependencies: []
tools:
  - name: text2image
    description: 提交文生图任务，返回 submit_id。需要用 query_result 查询生成状态，成功后返回图片 URL 或下载路径
    parameters:
      - name: prompt
        type: string
        required: true
        description: 生成图片的描述词（中文优先）
      - name: ratio
        type: string
        required: false
        default: "16:9"
        description: 图片比例，支持 21:9、16:9、3:2、4:3、1:1、3:4、2:3、9:16（默认 16:9）
      - name: resolution_type
        type: string
        required: true
        description: 清晰度/分辨率，取值根据模型版本决定（3.0/3.1 -> 1k 或 2k；4.0 及以上 -> 2k 或 4k；5.0Pro -> 1.5k、2k 或 4k）。默认推荐 2k（兼容最广）

  - name: image2image
    description: 提交图生图任务（以图作为参考生成新图），返回 submit_id
    parameters:
      - name: prompt
        type: string
        required: true
        description: 生成图片的描述词
      - name: image_path
        type: string
        required: true
        description: 参考图片的本地文件路径
      - name: ratio
        type: string
        required: false
        default: "1:1"
        description: 图片比例
      - name: resolution_type
        type: string
        required: true
        description: 清晰度/分辨率，取值根据模型版本决定（3.0/3.1 -> 1k 或 2k；4.0 及以上 -> 2k 或 4k；5.0Pro -> 1.5k、2k 或 4k）。默认推荐 2k（兼容最广）

  - name: image_upscale
    description: 提交图片放大任务，返回 submit_id
    parameters:
      - name: image_path
        type: string
        required: true
        description: 待放大的图片本地文件路径

  - name: text2video
    description: 提交文生视频任务，executor 自动轮询直到完成（最多 4 分钟），返回视频 URL 或路径
    parameters:
      - name: prompt
        type: string
        required: true
        description: 生成视频的描述词（中文优先）
      - name: duration
        type: number
        required: false
        default: 5
        description: 视频时长（秒），支持 4-30 秒，模型不同范围不同
      - name: ratio
        type: string
        required: false
        description: 视频比例，如 1:1、16:9、9:16
      - name: resolution_type
        type: string
        required: false
        default: "720p"
        description: 分辨率（内部映射为 --video_resolution），如 720p、1080p、4k

  - name: image2video
    description: 提交图生视频任务（以图为主画面生成动态视频），executor 自动轮询直到完成
    parameters:
      - name: prompt
        type: string
        required: true
        description: 视频动态描述词（中文优先）
      - name: image_path
        type: string
        required: true
        description: 主画面图片的本地文件路径
      - name: duration
        type: number
        required: false
        default: 5
        description: 视频时长（秒）
      - name: resolution_type
        type: string
        required: false
        default: "720p"
        description: 分辨率（内部映射为 --video_resolution），如 720p、1080p

  - name: frames2video
    description: 提交首尾帧视频任务（给定首帧和尾帧，生成过渡视频），executor 自动轮询直到完成
    parameters:
      - name: prompt
        type: string
        required: true
        description: 视频内容描述词（中文优先）
      - name: first_frame_path
        type: string
        required: true
        description: 首帧图片本地路径（参数名对应 CLI --first）
      - name: last_frame_path
        type: string
        required: true
        description: 尾帧图片本地路径（参数名对应 CLI --last）
      - name: duration
        type: number
        required: false
        default: 5
        description: 视频时长（秒）
      - name: ratio
        type: string
        required: false
        description: 视频比例，如 1:1、16:9、9:16

  - name: multiframe2video
    description: 提交多镜视频任务（2-N 张图片生成连贯视频），executor 自动轮询直到完成
    parameters:
      - name: image_paths
        type: string
        required: true
        description: 多张图片路径，逗号分隔，至少 2 张（对应 CLI --images）
      - name: prompt
        type: string
        required: false
        description: 过渡描述词（2 张时用 --prompt，多张时用 --transition-prompt）
      - name: duration
        type: number
        required: false
        default: 3
        description: 每段过渡时长（秒），总时长需 >= 2
      - name: resolution_type
        type: string
        required: false
        default: "720p"
        description: 分辨率，如 720p、1080p

  - name: multimodal2video
    description: 即梦旗舰视频模式（全能参考，支持图+视频+音频组合参考），executor 自动轮询直到完成
    parameters:
      - name: prompt
        type: string
        required: false
        description: 视频编辑提示词（可选）
      - name: reference_paths
        type: string
        required: false
        description: 参考素材路径，多个用逗号分隔，executor 按扩展名自动映射到 --image/--video/--audio
      - name: duration
        type: number
        required: false
        default: 5
        description: 视频时长（秒）
      - name: resolution_type
        type: string
        required: false
        default: "720p"
        description: 分辨率，如 720p、1080p

  - name: image_upscale
    description: 提交图片放大任务（参数名 image_path 对应 CLI --image），返回 submit_id
    parameters:
      - name: image_path
        type: string
        required: true
        description: 待放大的图片本地文件路径
      - name: resolution_type
        type: string
        required: false
        default: "2k"
        description: 放大分辨率，如 2k、4k、8k

  - name: user_credit
    description: 查询当前账号剩余积分
    parameters: []

  - name: list_task
    description: 列出最近的任务历史
    parameters:
      - name: gen_status
        type: string
        required: false
        description: 按状态过滤，如 success、fail、querying

  - name: query_result
    description: 根据 submit_id 查询任务结果，成功时返回图片/视频 URL 或文件路径
    parameters:
      - name: submit_id
        type: string
        required: true
        description: 提交任务时返回的 submit_id
      - name: download_dir
        type: string
        required: false
        description: 结果下载到指定目录

  - name: session_create
    description: 创建新的即梦 Session（用于组织创作历史）
    parameters:
      - name: name
        type: string
        required: false
        description: Session 名称（可选，不填自动命名）

  - name: session_list
    description: 列出最近的 Session 列表
    parameters: []

  - name: session_search
    description: 按名称搜索 Session ID
    parameters:
      - name: name
        type: string
        required: true
        description: Session 名称关键字

  - name: session_rename
    description: 重命名 Session
    parameters:
      - name: session_id
        type: string
        required: true
        description: Session ID
      - name: name
        type: string
        required: true
        description: 新名称

  - name: session_delete
    description: 删除 Session（历史自动移回默认 Session）
    parameters:
      - name: session_id
        type: string
        required: true
        description: Session ID

---

## 使用说明

对话中触发即梦生图/视频时，executor 会自动轮询直到完成（最长 4 分钟），直接返回结果给用户，无需手动调用 query_result。

**注意**：生成会消耗账号积分，请确认积分充足后再执行。

