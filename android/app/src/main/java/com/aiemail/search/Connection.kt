package com.aiemail.search

import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

class Connection internal constructor(val url: String, val token: String) {
    companion object {
        fun create(url: String, token: String, debug: Boolean, allowEmulatorHttp: Boolean): Connection {
            val parsed = url.trim().toHttpUrlOrNull() ?: throw IllegalArgumentException("请输入有效的服务器 URL")
            require(parsed.username.isEmpty() && parsed.password.isEmpty() && parsed.query == null && parsed.fragment == null) { "服务器 URL 不能包含用户名、密码、查询参数或片段" }
            require(parsed.isHttps || (debug && allowEmulatorHttp && parsed.scheme == "http" && parsed.host == "10.0.2.2")) { "服务器必须使用 HTTPS；调试版可显式允许模拟器 10.0.2.2" }
            require(token.isNotEmpty() && token.all { it.code in 33..126 }) { "请输入有效的访问令牌，不能包含空格或换行" }
            return Connection(parsed.toString().trimEnd('/') + "/", token)
        }
    }
}
