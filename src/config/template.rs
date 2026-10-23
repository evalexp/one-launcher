/// 生成包含详细中文注释的默认 config.toml 文本内容
pub fn default_config_content() -> &'static str {
    r#"# ==============================================================================
# OneLauncher 极速启动器配置文件
# 说明：本文件修改保存后会自动热重载生效，无需重启程序。
# ==============================================================================

[general]
# 全局呼出/隐藏快捷键
# 支持修饰键: Alt, Ctrl, Shift, Win，用 "+" 连接键名
# 示例: "Alt+Space", "Ctrl+Space", "Win+Space", "F12"
hotkey = "Alt+Space"

# 窗口失去焦点时是否自动隐藏
hide_on_blur = true

# 呼出窗口时是否清空上一次搜索框中的文本
clear_on_show = true

# 搜索结果列表最大展示数量 (推荐 7 ~ 12)
max_results = 9

[appearance]
# 主题外观: "system" (跟随系统), "dark" (深色模式), "light" (浅色模式)
theme = "system"

# 启动器窗口宽度 (像素)
width = 680

# 基础字体大小 (像素)
font_size = 16

# 窗口背景特效 (Windows 11 支持 mica 和 acrylic，Windows 10 支持 acrylic 和 solid)
# 可选值: "mica", "acrylic", "solid"
backdrop_effect = "mica"

[apps]
# 需要索引的文件扩展名
extensions = ["lnk", "exe", "url"]

# 是否扫描 Windows 10/11 UWP (应用商店及系统内置) 应用程序
scan_uwp = true

# 额外扫描的自定义软件目录 (例如便携绿色软件目录、开发工具箱目录)
# Windows 路径请注意使用双反斜杠 "\\" 或正斜杠 "/"
extra_directories = [
    # "D:\\PortableApps",
    # "D:\\Tools",
]

# 排除的黑名单关键词或文件名（匹配到则不收录进搜索结果）
exclude_patterns = [
    "卸载",
    "Uninstall",
    "Help",
    "Readme",
    "Documentation",
    "安装向导",
]

[everything]
# 是否开启 Everything 联动搜索
enabled = true

# 专属搜索前缀
# 留空 "" 表示混合搜索模式 (应用结果优先，其后展示 Everything 文件结果)
# 设置如 "f " 或 "e:" 则只有在输入该前缀时才触发 Everything 搜索
prefix = ""

# Everything 实例名称 (空字符串 "" 为默认实例，若使用 Everything 1.5 alpha 多实例可填写实例名)
instance_name = ""

# Everything 文件搜索返回的最大结果条数
max_results = 20
"#
}
