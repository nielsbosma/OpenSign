# 快速入门

本页带你从零开始，在自己的机器上完成一份文档的签署。要用于生产环境，请接着阅读 [部署](deployment.md)。

## 1. 在本地运行 OpenSign

你需要 Docker 和一份仓库副本。

```sh
git clone https://github.com/nielsbosma/OpenSign.git
cd OpenSign
docker compose up -d --wait postgres s3 mailpit converter
docker compose run --rm s3-init
docker compose --profile prod up -d --build
```

前两条命令会启动 Postgres、兼容 S3 的存储、用于 Word 文件的 Gotenberg 和 Mailpit，并创建存储桶。最后一条命令构建并启动生产镜像。

- OpenSign：http://localhost:8090
- Mailpit，开发用的本地收件箱：http://localhost:8026

生产镜像会发送真实邮件，所以需要一个真实的邮箱账户（第 3 步）。如果想在没有邮箱账户的情况下试用 OpenSign，可以改用开发模式运行，所有邮件都会进入 Mailpit：

```sh
pnpm install
pnpm infra:up
cp .env.example .env
pnpm dev            # web app on http://localhost:5173
```

## 2. 创建第一个管理员

全新安装会在日志中打印一个**设置密钥**，这样在你之前没有其他人能抢先认领。

```sh
docker compose logs app | grep "setup code"
```

在开发模式下，它出现在运行 `pnpm dev` 的终端里。

打开 OpenSign，输入设置密钥、你的姓名、组织、邮箱和密码。每一步的作用参见 [首次运行设置与邮件](setup.md)。

## 3. 连接邮件

OpenSign 通过邮件发送邀请、验证码和已签署的文档，所以在任何人发送文档之前，它都会要求你配置邮件提供商。输入 Resend API 密钥，以及你在 Resend 中验证过的域名下的发件地址。OpenSign 会给你发一封测试邮件，发送成功后才会继续。

在开发模式下，你可以让密钥留空：测试邮件及之后的所有邮件都会进入 Mailpit。

## 4. 发送文档

1. 进入**文档**，选择**新建文档**。
2. 拖入 PDF 或 Word 文件，填写标题并创建。
3. 在**参与方**下，把你自己添加为签署人，填入你的邮箱地址。
4. 在字段工具栏中选择该参与方，在页面上放置一个**签名**字段。
5. 选择**发起签署**。

打开邀请邮件，点击链接，手写或输入签名并签署。几秒钟后，文档变为**已签署**，你可以在文档页面下载封印后的 PDF。

## 5. 检查结果

在侧边栏打开**验证文档**（或访问 `/verify`），拖入封印后的 PDF。OpenSign 会检查印章和时间戳，并确认它封印的正是这份文件。能识别签名的 PDF 阅读器（例如 Adobe Acrobat）也会显示该印章。

## 后续步骤

- [部署](deployment.md)：真正投入运行，在 Sliplane 或任何能运行容器的地方。
- [身份验证](identification.md)：设置短信验证码、BankID 或 Freja。
- [用户与角色](users-and-roles.md)：邀请你的同事。
- [REST API](api.md)：从你自己的系统发送文档。
