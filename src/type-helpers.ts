export type RpcHandler = ParentHandler | LeafHandler;

export type ParentHandler = {
  _CHILDREN: {
    [name: string]: RpcHandler;
  };
  _PARAMS: unknown;
  _RETURN?: unknown;
};

export type LeafHandler = {
  _PARAMS: unknown;
  _RETURN: unknown;
};

// An empty method resolves to the node itself (a directly-callable handler, e.g. a parent
// with a root handler). A single segment recurses into that child with an empty method, which
// unifies leaf children and directly-callable parent children.
export type RpcParamType<
  Root extends RpcHandler,
  Method extends string
> = Method extends ""
  ? Root["_PARAMS"]
  : Root extends ParentHandler
  ? Method extends `${infer A}.${infer B}`
    ? Root["_PARAMS"] & RpcParamType<Root["_CHILDREN"][A], B>
    : Method extends keyof Root["_CHILDREN"]
    ? Root["_PARAMS"] & RpcParamType<Root["_CHILDREN"][Method], "">
    : never
  : never;

export type RpcReturnType<
  Root extends RpcHandler,
  Method extends string
> = Method extends ""
  ? Root["_RETURN"]
  : Root extends ParentHandler
  ? Method extends `${infer A}.${infer B}`
    ? RpcReturnType<Root["_CHILDREN"][A], B>
    : Method extends keyof Root["_CHILDREN"]
    ? RpcReturnType<Root["_CHILDREN"][Method], "">
    : never
  : never;
