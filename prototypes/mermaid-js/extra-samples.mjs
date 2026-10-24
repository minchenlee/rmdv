export const extraSamples = [
  { title: 'AWS 圖示流程圖', source: `flowchart LR
  api@{ icon: "aws:arch-amazon-api-gateway", label: "API Gateway", form: "square", pos: "b" }
  fn@{ icon: "aws:arch-aws-lambda", label: "Lambda", form: "square", pos: "b" }
  db@{ icon: "aws:arch-amazon-dynamodb", label: "DynamoDB", form: "square", pos: "b" }
  api --> fn --> db
` },
  { title: 'AWS 架構圖', source: `architecture-beta
  group backend(cloud)[Serverless API]
  service api(aws:arch-amazon-api-gateway)[API Gateway] in backend
  service fn(aws:arch-aws-lambda)[Lambda] in backend
  service db(aws:arch-amazon-dynamodb)[DynamoDB] in backend
  api:R --> L:fn
  fn:R --> L:db
` },
];
