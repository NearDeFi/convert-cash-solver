declare module '@neardefi/shade-agent-js' {
    export function agentCall(params: any): Promise<any>;
    export function agentView(params: any): Promise<any>;
    export function agentAccountId(): Promise<{ accountId: string }>;
    export function contractCall(params: any): Promise<any>;
}

declare module '../app.js' {
    export function callWithAgent(params: {
        contractId?: string;
        methodName: string;
        args: any;
        gas?: string;
        deposit?: string;
    }): Promise<any>;
}
