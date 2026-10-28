import { getLogger } from '@platformatic/globals'
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http'

export function create() {
  const logger = getLogger()
  
  return createServer((_: IncomingMessage, res: ServerResponse) => {
    logger.debug('Serving request.')
    res.writeHead(200, { 'content-type': 'application/json', connection: 'close' })
    res.end(JSON.stringify({ hello: 'world' }))
  })
}
