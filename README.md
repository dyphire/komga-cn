# ![app icon](https://github.com/gotson/komga/raw/master/.github/readme-images/app-icon.png) Komga

[![GitHub release (latest SemVer)](https://img.shields.io/github/v/release/dyphire/komga-cn?color=blue&label=download&sort=semver)](https://github.com/dyphire/komga-cn/releases/latest)
[![GitHub all releases](https://img.shields.io/github/downloads/dyphire/komga-cn/total?color=blue&label=github%20downloads)](https://github.com/dyphire/komga-cn/releases)
[![Docker Pulls](https://img.shields.io/docker/pulls/dyphire/komga-cn)](https://hub.docker.com/r/dyphire/komga-cn)

在原版基础上进行了增强：

- 对 MOBI 格式的漫画做了支持
- 增强封面获取逻辑
- 支持中文拼音首字母索引（需要使用本镜像建立 "库"）
- 支持繁体自动转换为简体
- 增强 EPUB 漫画类的启发式检测
- 支持入库扫描时移除广告页（基于二维码检测）
- 添加智能筛选器支持
- 搜索页面改为标签页 + 图库视图，支持按库查看搜索结果
- 增强双页阅读器
- 在 EPUB 阅读器中添加进度标记切换
- 在 Divina 阅读器中添加 EPUB 目录支持
- 在系列和书籍的上下文菜单中添加下载功能
- 批量编辑书籍时预填充作者选择


github: https://github.com/dyphire/komga-cn

## docker run

```
docker run -d \
    --name komga-cn \
    -v ./config:/config \
    -m 4096m \
    -p 25600:25600  \
    -e TZ=Asia/Shanghai \
    -e CHS=TRUE \
    --restart always \
    dyphire/komga-cn:latest
```

## docker compose

```
version: "3"
services:
  komga-cn:
    image: dyphire/komga-cn:latest
    container_name: komga-cn
    network_mode: bridge
    mem_limit: 4096m
    ports:
      - "25600:25600" # web端口
    environment:
      - TZ=Asia/Shanghai
      - CHS=TRUE # 开启繁转简
    volumes:
      - ./config:/config # 配置文件存放位置
    restart: always
```
