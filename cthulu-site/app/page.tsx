import Nav from "@/components/Nav";
import Hero from "@/components/Hero";
import ValueProps from "@/components/ValueProps";
import MultiAgent from "@/components/MultiAgent";
import UseCases from "@/components/UseCases";
import HowItWorks from "@/components/HowItWorks";
import StudioShowcase from "@/components/StudioShowcase";
import ConfigExample from "@/components/ConfigExample";
import GetStarted from "@/components/GetStarted";
import Footer from "@/components/Footer";

export default function Home() {
  return (
    <>
      <Nav />
      <main>
        <Hero />
        <ValueProps />
        <MultiAgent />
        <UseCases />
        <HowItWorks />
        <StudioShowcase />
        <ConfigExample />
        <GetStarted />
      </main>
      <Footer />
    </>
  );
}
