export default function Home() {
  return (
    <main className="main">
      <div className="description">
        <p>
          Get started by editing&nbsp;
          <code className="code">app/page.tsx</code>
        </p>
      </div>

      <div className="center">
        <h1>Next.js App Router</h1>
        <p>Edit this page and save to see updates.</p>
      </div>

      <div className="grid">
        <a
          className="card"
          href="https://nextjs.org/docs"
          target="_blank"
          rel="noopener noreferrer"
        >
          <h2>
            Docs <span>-&gt;</span>
          </h2>
          <p>Find in-depth information about Next.js features and API.</p>
        </a>

        <a
          className="card"
          href="https://nextjs.org/learn"
          target="_blank"
          rel="noopener noreferrer"
        >
          <h2>
            Learn <span>-&gt;</span>
          </h2>
          <p>Learn about Next.js in an interactive course with quizzes!</p>
        </a>

        <a
          className="card"
          href="https://vercel.com/templates"
          target="_blank"
          rel="noopener noreferrer"
        >
          <h2>
            Templates <span>-&gt;</span>
          </h2>
          <p>Explore starter templates for Next.js.</p>
        </a>

        <a
          className="card"
          href="https://vercel.com/new"
          target="_blank"
          rel="noopener noreferrer"
        >
          <h2>
            Deploy <span>-&gt;</span>
          </h2>
          <p>Instantly deploy your Next.js site to a shareable URL.</p>
        </a>
      </div>
    </main>
  );
}
