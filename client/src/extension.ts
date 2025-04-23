import * as path from "path";
import { ExtensionContext } from "vscode";

import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient;

export function activate(context: ExtensionContext) {
    const serverModule = context.asAbsolutePath(
        path.join("server", "target", ...(process.platform === "darwin" ? ["release", "vue-property-decorator-extension-server"] : process.platform === "win32" ? ["x86_64-pc-windows-gnu", "release", "vue-property-decorator-extension-server.exe"] : ["x86_64-unknown-linux-musl", "release", "vue-property-decorator-extension-server"]))
    );

    const serverOptions: ServerOptions = {
        run: { command: serverModule },
        debug: {
            command: "cargo",
            args: ["run"],
            options: {
                cwd: context.asAbsolutePath("server"),
            },
        },
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: "file", language: "typescript" }, { scheme: "file", language: "vue" }],
        progressOnInitialization: true,
    };

    // Create the language client and start the client.
    client = new LanguageClient(
        "vue-property-decorator-extension",
        "Vue Decorator Language Service",
        serverOptions,
        clientOptions
    );

    // Start the client. This will also launch the server
    client.start();
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
