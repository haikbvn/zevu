import { useEffect, useState } from "react";

export default function Page() {
  const [license, setLicense] = useState<string | undefined>();
  useEffect(() => {
    fetch("/license/NOTICE.txt")
      .then((response) => {
        if (!response.ok) {
          throw new Error("HTTP error " + response.status);
        }
        return response.text();
      })
      .then((text) => {
        setLicense(text);
      })
      .catch((e) => {
        console.error(e);
      });
  }, []);

  return (
    <>
      <section className={`flex flex-col py-4`}>
        <h2
          id={`subhead-licenses`}
          className="font-bold text-medium text-zinc-400 leading-none mt-2"
        >
          Licenses
        </h2>
        <div className={`flex p-4 pl-0 gap-4 whitespace-pre-wrap`}>
          {license}
        </div>
      </section>
    </>
  );
}
